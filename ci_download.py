"""Robust, isolated GitHub Actions artifact retrieval.

A missing or transiently unavailable artifact must not abort the entire
disaster-recovery cycle. Exhaust retries, then let the parent mark just this CI
run incomplete so its checkpoint remains retryable on the next resume.
"""
from __future__ import annotations

import shutil
import time
import zipfile
from pathlib import Path


class ArtifactDownloadError(RuntimeError):
    """A single requested Actions artifact was not preserved."""


def retrieve_artifact(slug, run_id, artifact, inventory, directory, gh,
                      *, sleeper=time.sleep, delays=(1, 3, 7), on_retry=None):
    """Retrieve one artifact as a ZIP or extracted tree, without trusting names.

    gh is the existing authenticated GitHub CLI adapter. Intermediate failed
    attempts are quiet; a final explicit exception records lost completeness.
    Each retry starts from an empty private output, never appending corrupt bytes.
    """
    artifact_id = artifact.get('id')
    name = artifact.get('name')
    if type(artifact_id) is not int or artifact_id <= 0:
        raise ArtifactDownloadError('Invalid GitHub Actions artifact ID')
    if type(run_id) is not int or run_id <= 0:
        raise ArtifactDownloadError('Invalid GitHub Actions workflow run ID')
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    target = directory / (str(artifact_id) + '.zip')
    # gh api is fast when GitHub follows the binary redirect; otherwise
    # gh run download is the supported authenticated fallback.
    with target.open('wb') as output:
        result = gh(['api', f'/repos/{slug}/actions/artifacts/{artifact_id}/zip'],
                    f'CI artifact {artifact_id}', optional=True, quiet=True,
                    file=output, timeout=3600)
    if result is not None and target.stat().st_size >= 22 and zipfile.is_zipfile(target):
        return target
    target.unlink(missing_ok=True)

    # --name is ambiguous if a run carries multiple matching artifact names.
    matches = [item for item in inventory
               if item.get('name') == name and item.get('expired') is False]
    if not isinstance(name, str) or not name or len(matches) != 1:
        raise ArtifactDownloadError(
            f'Artifact {artifact_id}: binary download invalid and fallback name is ambiguous')
    extracted = directory / ('extracted-' + str(artifact_id))
    for attempt in range(len(delays) + 1):
        if extracted.exists():
            shutil.rmtree(extracted)
        extracted.mkdir(mode=0o700)
        result = gh(['run', 'download', str(run_id), '--repo', slug,
                     '--name', name, '--dir', str(extracted)],
                    f'CI artifact fallback {artifact_id}',
                    optional=True, quiet=True, timeout=3600)
        if result is not None:
            files = [p for p in extracted.rglob('*') if p.is_file()]
            # Never archive symlinks or paths which resolve outside staging.
            if files and all(not p.is_symlink() and
                             p.resolve().is_relative_to(extracted.resolve())
                             for p in files):
                return extracted
        if attempt < len(delays):
            if on_retry is not None:
                on_retry(artifact_id, attempt + 1, len(delays))
            sleeper(delays[attempt])
    shutil.rmtree(extracted, ignore_errors=True)
    raise ArtifactDownloadError(
        f'Artifact {artifact_id} ({name!r}) could not be downloaded after '
        f'{len(delays) + 1} fallback attempts; this run remains retryable')
