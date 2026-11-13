"""Read, safely extract, checkout and mount encrypted backup archives."""
from __future__ import annotations
import contextlib
import io
import json
import os
import re
import shutil
import subprocess
import tarfile
import tempfile
from pathlib import Path, PurePosixPath
from checkpoint import sha256

class ArchiveError(RuntimeError):
    pass


def _safe_member(member):
    p = PurePosixPath(member.name)
    if p.is_absolute() or not p.parts or any(x in ('..', '', '.') for x in p.parts):
        raise ArchiveError('Unsafe archive member path: ' + member.name)
    if not (member.isfile() or member.isdir()):
        raise ArchiveError('Unsafe archive member type: ' + member.name)
    return p

@contextlib.contextmanager
def open_encrypted_archive(path: Path, identity: Path | None = None):
    if not path.is_file() or path.is_symlink() or path.suffix != '.age':
        raise ArchiveError('Not an encrypted .age archive: ' + str(path))
    command = ['age', '--decrypt']
    if identity:
        if not identity.is_file():
            raise ArchiveError('Age identity file not found')
        command += ['-i', str(identity)]
    else:
        raise ArchiveError('Use --identity with an age private key to browse encrypted archives')
    command.append(str(path))
    proc = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        assert proc.stdout is not None
        yield tarfile.open(fileobj=proc.stdout, mode='r|gz')
        proc.stdout.close()
        error = proc.stderr.read().decode('utf-8', 'replace')
        if proc.wait() != 0:
            raise ArchiveError('Failed to decrypt/read archive: ' + error[:500])
    finally:
        if proc.poll() is None:
            proc.kill()
        proc.wait()
        if proc.stdout: proc.stdout.close()
        if proc.stderr: proc.stderr.close()


def list_members(path: Path, identity: Path, *, limit=250000):
    entries = []
    with open_encrypted_archive(path, identity) as archive:
        for index, m in enumerate(archive):
            _safe_member(m)
            if index >= limit:
                raise ArchiveError('Archive contains too many members')
            entries.append({'path': m.name, 'bytes': m.size, 'directory': m.isdir()})
    return entries


def read_member(path: Path, identity: Path, name: str, *, max_bytes=1024*1024):
    """Read one member and consume the full encrypted stream before closing age.

    Exiting a streaming tar reader as soon as a member is found would SIGPIPE
    the decryptor and incorrectly report a failure for a valid archive.
    """
    found = None
    with open_encrypted_archive(path, identity) as archive:
        for m in archive:
            _safe_member(m)
            if m.name != name:
                continue
            if not m.isfile() or m.size > max_bytes:
                raise ArchiveError('Entry is not a file or exceeds display limit')
            stream = archive.extractfile(m)
            if stream is None:
                raise ArchiveError('Cannot read entry')
            found = stream.read(max_bytes + 1).decode('utf-8', 'replace')
    if found is None:
        raise ArchiveError('Entry not found: ' + name)
    return found


def extract_archive(path: Path, identity: Path, destination: Path, *, max_members=500000,
                    max_bytes=100 * 1024**3):
    dest = destination.resolve()
    dest.mkdir(parents=True, exist_ok=True, mode=0o700)
    total = 0
    with open_encrypted_archive(path, identity) as archive:
        for index, member in enumerate(archive):
            relative = _safe_member(member)
            if index >= max_members: raise ArchiveError('Too many entries')
            total += member.size
            if total > max_bytes: raise ArchiveError('Archive exceeds extraction size limit')
            current = dest.joinpath(*relative.parts)
            if not current.resolve().is_relative_to(dest):
                raise ArchiveError('Archive path escapes destination')
            if current.is_symlink():
                raise ArchiveError('Existing symlink in destination')
            if member.isdir():
                current.mkdir(parents=True, exist_ok=True, mode=0o700)
                continue
            current.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
            # O_EXCL prevents archive members from overwriting existing files.
            with current.open('xb') as fp:
                content = archive.extractfile(member)
                if content is None: raise ArchiveError('Missing member content')
                shutil.copyfileobj(content, fp, length=1024 * 1024)
            current.chmod(0o600)
    return dest


def catalog(root: Path):
    result = []
    for kind, folder in [('git', root / 'repositories'), ('metadata', root / 'metadata'),
                         ('legacy', root / '_metadata'),
                         ('dropbox', root / 'dropbox' / 'objects'),
                         ('dropbox-manifest', root / 'dropbox' / 'manifests')]:
        if not folder.is_dir(): continue
        for path in sorted(folder.rglob('*.tar.gz.age'), key=lambda p: p.stat().st_mtime, reverse=True):
            if path.is_symlink(): continue
            relative = path.relative_to(root).as_posix()
            if kind == 'git' and '__git-' not in path.name: continue
            result.append({'kind': kind, 'path': str(path), 'relative': relative,
                           'size': path.stat().st_size})
    return result


def checkout_git(path: Path, identity: Path, destination: Path):
    if destination.exists():
        raise ArchiveError('Checkout destination must not already exist')
    with tempfile.TemporaryDirectory(prefix='multi-backup-checkout-') as temp:
        extract_archive(path, identity, Path(temp))
        bare = sorted(Path(temp).glob('multi-backup/repositories/*/*/git/*.git')) + sorted(Path(temp).glob('github-backup/repositories/*/*/git/*.git'))
        bare = [p for p in bare if not p.name.endswith('.wiki.git')]
        if len(bare) != 1:
            raise ArchiveError('Expected exactly one Git mirror in archive')
        result = subprocess.run(['git', 'clone', '--no-hardlinks', str(bare[0]), str(destination)],
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
        if result.returncode:
            shutil.rmtree(destination, ignore_errors=True)
            raise ArchiveError('Git checkout failed: ' + result.stderr.decode('utf-8','replace')[:500])
    return destination


def mount_archive(path: Path, identity: Path, mountpoint: str, workspace: Path):
    """Read-only bindfs on macOS/Linux; Windows maps a temporary checkout with subst."""
    mounts = workspace / 'mounts'; mounts.mkdir(parents=True, exist_ok=True, mode=0o700)
    key = re.sub(r'[^a-zA-Z0-9_-]', '_', sha256(path)[:20])
    source = mounts / key / 'content'
    if source.exists():
        raise ArchiveError('This archive is already staged for mounting; unmount first')
    source.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    try:
        if '/repositories/' in str(path) and '__git-' in path.name:
            checkout_git(path, identity, source)
        else:
            extract_archive(path, identity, source)
        if os.name == 'nt':
            if not re.fullmatch(r'[A-Za-z]:', mountpoint):
                raise ArchiveError('Windows mountpoint must be an unused drive letter, e.g. X:')
            command = ['subst', mountpoint, str(source)]
        else:
            if not shutil.which('bindfs'):
                raise ArchiveError('bindfs is required for a read-only FUSE mount on this OS')
            point = Path(mountpoint)
            point.mkdir(parents=True, exist_ok=True)
            command = ['bindfs', '-r', str(source), str(point)]
        completed = subprocess.run(command, capture_output=True, check=False)
        if completed.returncode:
            raise ArchiveError('Mount failed: ' + completed.stderr.decode('utf-8','replace')[:500])
        (source.parent / 'mount.json').write_text(json.dumps({'mountpoint': mountpoint, 'path': str(path)}))
        return str(source.parent)
    except Exception:
        shutil.rmtree(source.parent, ignore_errors=True)
        raise


def unmount_archive(workspace: Path, mountpoint: str):
    for manifest in (workspace / 'mounts').glob('*/mount.json'):
        try: obj = json.loads(manifest.read_text())
        except Exception: continue
        if obj.get('mountpoint') != mountpoint: continue
        if os.name == 'nt': cmd = ['subst', mountpoint, '/D']
        else:
            cmd = ['fusermount', '-u', mountpoint] if shutil.which('fusermount') else ['umount', mountpoint]
        result = subprocess.run(cmd, capture_output=True)
        if result.returncode: raise ArchiveError('Unmount failed; staging preserved')
        shutil.rmtree(manifest.parent)
        return
    raise ArchiveError('Unknown mountpoint')
