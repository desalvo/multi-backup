"""Single-pass, fail-closed migration of legacy global age/tar metadata.

Old archives are never removed by migration. Only the currently processed legacy
archive is decrypted into a private, encrypted-volume staging directory and
then repackaged into independent encrypted owner/repository archives.
"""
from __future__ import annotations
import hashlib
import json
import os
import shutil
import tempfile
from pathlib import Path
from archive_ops import open_encrypted_archive, _safe_member, ArchiveError
from checkpoint import atomic_json, sha256
from github_disaster_backup import StreamingArchive


def legacy_archives(output: Path):
    paths = list((output / '_metadata').glob('github-disaster-metadata-*.tar.gz.age'))
    paths += list(output.glob('github-disaster-*.tar.gz.age'))
    return sorted(p for p in paths if p.is_file() and not p.is_symlink())


def migrate_legacy(output: Path, staging: Path, identity: Path | None, recipient: str | None, progress):
    paths = legacy_archives(output)
    if not paths:
        return []
    if identity is None or recipient is None:
        raise ArchiveError('Legacy monolithic metadata exist. Pass --identity and --recipient to migrate them safely.')
    migrations = output / 'metadata' / '_migrations'
    migrations.mkdir(parents=True, exist_ok=True, mode=0o700)
    results = []
    for i, source in enumerate(paths, 1):
        fingerprint = sha256(source)
        marker = migrations / (fingerprint + '.json')
        if marker.exists():
            record = json.loads(marker.read_text('utf-8'))
            if record.get('source_sha256') == fingerprint and all(
                Path(v['path']).is_file() and sha256(Path(v['path'])) == v['sha256']
                for v in record.get('shards', {}).values()
            ):
                results.append(record)
                progress.emit('migration', 'Already migrated: '+source.name, completed=i,total=len(paths))
                continue
        progress.emit('migration', 'Repartitioning: '+source.name,completed=i-1,total=len(paths))
        with tempfile.TemporaryDirectory(dir=staging, prefix='.migration-') as work:
            temp = Path(work)
            groups = {}
            try:
                with open_encrypted_archive(source, identity) as archive:
                    for index, member in enumerate(archive):
                        rel = _safe_member(member)
                        parts = rel.parts
                        if (len(parts) >= 4 and parts[:2] in (('multi-backup', 'repositories'),('github-backup', 'repositories'))
                                and parts[2] not in ('.','..') and parts[3] not in ('.','..')):
                            group = 'repositories/' + parts[2] + '/' + parts[3]
                        else:
                            group = 'common'
                        groups[group] = groups.get(group, 0) + 1
                        target = temp.joinpath(group, *parts)
                        if not target.resolve().is_relative_to(temp.resolve()):
                            raise ArchiveError('Unsafe legacy member')
                        if member.isdir():
                            target.mkdir(parents=True, exist_ok=True, mode=0o700)
                        elif member.isfile():
                            target.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
                            # Legacy streams may contain repeated indices. The last wins.
                            with target.open('wb') as fp:
                                src = archive.extractfile(member)
                                if src: shutil.copyfileobj(src, fp, length=1024*1024)
                            target.chmod(0o600)
                shards = {}
                for group in sorted(groups):
                    if group == 'common':
                        destination = output / 'metadata' / 'legacy' / fingerprint[:16] / 'common.tar.gz.age'
                    else:
                        destination = output / 'metadata' / 'legacy' / fingerprint[:16] / group / 'metadata.tar.gz.age'
                    destination.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
                    pending = destination.with_name(destination.name + '.partial')
                    try:
                        with StreamingArchive(pending, recipient) as stream:
                            for entry in sorted((temp / group).iterdir()):
                                stream.add_tree(entry, entry.name)
                        os.replace(pending, destination)
                    finally:
                        pending.unlink(missing_ok=True)
                    shards[group] = {'path':str(destination), 'sha256':sha256(destination)}
                record = {'source':str(source), 'source_sha256':fingerprint,
                          'shards':shards, 'format':'per-repository-metadata-v1'}
                atomic_json(marker, record)
                results.append(record)
            except Exception:
                # The old immutable backup is always preserved for retry.
                raise
        progress.emit('migration', 'Migrated: '+source.name, completed=i,total=len(paths))
    return results
