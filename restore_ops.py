"""Plugin-matched, explicitly confirmed restores; never silently overwrite remote data."""
from __future__ import annotations

import json
import os
import re
import ssl
import shutil
import subprocess
import tempfile
import urllib.error
import urllib.request
from pathlib import Path

from archive_ops import ArchiveError, extract_archive
from checkpoint import sha256
import certifi

GITHUB_SLUG=re.compile(r'^[a-zA-Z0-9][a-zA-Z0-9-]{0,38}/[a-zA-Z0-9_.-]{1,100}$')
DROPBOX_PATH=re.compile(r'^/(?!/)(?!.*(?:^|/)\.\.(?:/|$))[^\\\x00-\x1f]+$')
DIRECT_UPLOAD_LIMIT=150*1024*1024


class RestoreError(RuntimeError):
    pass


def restore_provider(path, backup_root):
    """Derive provider from immutable backup storage layout, never from UI selection."""
    if Path(path).is_symlink():
        raise RestoreError('Symlinked archives cannot be restored')
    archive=Path(path).resolve()
    root=Path(backup_root).resolve()
    if archive.suffix!='.age' or not archive.is_file() or archive.is_symlink():
        raise RestoreError('Choose a valid encrypted backup archive')
    try:
        relative=archive.relative_to(root)
    except ValueError as exc:
        raise RestoreError('Archive is outside the selected backup destination') from exc
    top=relative.parts[0] if relative.parts else ''
    if top in ('repositories','metadata','_metadata'):
        return 'github'
    if top=='dropbox':
        return 'dropbox'
    raise RestoreError('Cannot identify the backup plugin; restore blocked')


def validate_plugin(path, root, selected):
    actual=restore_provider(path,root)
    if selected!=actual:
        raise RestoreError(
            'Incompatible backup: '+actual+' archives cannot be restored with '+selected+' plugin')
    return actual


def restore_local(path,root,plugin,identity,destination):
    validate_plugin(path,root,plugin)
    destination=Path(destination).expanduser()
    if destination.is_symlink() or destination.exists():
        raise RestoreError('Choose a new, nonexisting restore destination')
    if not destination.parent.is_dir():
        raise RestoreError('Restore destination parent must exist')
    if plugin=='github' and '/repositories/' in str(Path(path)):
        from archive_ops import checkout_git
        return checkout_git(Path(path),Path(identity),destination)
    return extract_archive(Path(path),Path(identity),destination)


def _checked_run(argv, *,cwd=None,timeout=1800):
    p=subprocess.run(argv,cwd=cwd,capture_output=True,text=True,
                     timeout=timeout,check=False)
    if p.returncode:
        raise RestoreError((p.stderr or p.stdout or 'Command failed')[:800])
    return p.stdout.strip()


def restore_github_new_repository(archive,root,identity,repo,*,private=True):
    """Restore exactly one Git mirror to a NEW GitHub repository.

    Requires gh auth to already be configured; never pushes to an existing repo.
    """
    validate_plugin(archive,root,'github')
    archive=Path(archive)
    if '/repositories/' not in str(archive) or '__git-' not in archive.name:
        raise RestoreError('Select an individual Git mirror, not a metadata archive')
    if not GITHUB_SLUG.fullmatch(repo) or '..' in repo:
        raise RestoreError('Invalid GitHub owner/repository name')
    login=_checked_run(['gh','api','user','--jq','.login'],timeout=60)
    if repo.split('/',1)[0].lower()!=login.lower():
        raise RestoreError('Restore must use the authenticated GitHub owner')
    existing=subprocess.run(['gh','repo','view',repo,'--json','name'],
                            capture_output=True,text=True,timeout=60,check=False)
    if existing.returncode==0:
        raise RestoreError('Target GitHub repository already exists; refusing to overwrite')
    with tempfile.TemporaryDirectory(prefix='multi-backup-github-restore-') as tmp:
        stage=Path(tmp)/'extracted'
        extract_archive(archive,Path(identity),stage)
        mirrors=list(stage.glob('multi-backup/repositories/*/*/git/*.git'))
        mirrors+=list(stage.glob('github-backup/repositories/*/*/git/*.git'))
        mirrors=[m for m in mirrors if not m.name.endswith('.wiki.git')]
        if len(mirrors)!=1:
            raise RestoreError('Expected exactly one GitHub mirror in archive')
        # The authenticated user explicitly approved creation of a NEW repository.
        _checked_run(['gh','repo','create',repo,'--private' if private else '--public'],
                     timeout=120)
        _checked_run(['gh','auth','setup-git'],timeout=60)
        remote='https://github.com/'+repo+'.git'
        _checked_run(['git','-C',str(mirrors[0]),'push','--mirror',remote],timeout=3600)
        # LFS push is attempted when objects are present. Never falsely assert success
        # for a snapshot with missing content.
        _checked_run(['git','-C',str(mirrors[0]),'lfs','push','--all',remote],timeout=3600)
    return 'https://github.com/'+repo


def _dropbox_upload(endpoint,token,argument,data,*,required='id'):
    # Constant HTTPS Dropbox API host and endpoint controlled by this module.
    if endpoint not in ('upload','upload_session/start',
                        'upload_session/append_v2','upload_session/finish'):
        raise RestoreError('Unrecognized Dropbox upload endpoint')
    request=urllib.request.Request(
        'https://content.dropboxapi.com/2/files/'+endpoint,
        data=data,
        headers={'Authorization':'Bearer '+token,
                 'Content-Type':'application/octet-stream',
                 'Dropbox-API-Arg':json.dumps(argument,separators=(',',':'))},
        method='POST')
    class RefuseRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self,req,fp,code,msg,headers,newurl):
            raise RestoreError('Dropbox upload redirect blocked to protect the write token')

    try:
        # Strictly prevent cross-origin redirects with the temporary write token.
        opener=urllib.request.build_opener(
            urllib.request.HTTPSHandler(
                context=ssl.create_default_context(cafile=certifi.where())),
            RefuseRedirect())
        with opener.open(request,timeout=180) as response:
            payload=response.read(1024*1024)
            result=json.loads(payload) if payload else {}
    except urllib.error.HTTPError as exc:
        if exc.code==409:
            raise RestoreError('Dropbox destination exists or conflicts; it was not overwritten') from exc
        raise RestoreError('Dropbox restore failed (HTTP '+str(exc.code)+')') from exc
    if not isinstance(result,dict) or (required and not result.get(required)):
        raise RestoreError('Dropbox did not confirm upload operation')
    return result


def restore_dropbox_new_file(archive,root,identity,destination,token):
    """Restore any Dropbox backup file to a new path without overwriting data."""
    validate_plugin(archive,root,'dropbox')
    source=Path(archive).resolve()
    if '/objects/' not in str(source) or not source.name.endswith('.tar.gz.age'):
        raise RestoreError('Select an individual Dropbox object, not its manifest')
    if not DROPBOX_PATH.fullmatch(destination) or destination.endswith('/'):
        raise RestoreError('Use an absolute Dropbox file path beginning with /')
    if not isinstance(token,str) or len(token)<12 or '\n' in token:
        raise RestoreError('A Dropbox token with files.content.write permission is required')
    with tempfile.TemporaryDirectory(prefix='multi-backup-dropbox-restore-') as tmp:
        decoded=Path(tmp)/'decoded'
        extract_archive(source,Path(identity),decoded)
        files=[p for p in decoded.rglob('*') if p.is_file()]
        if len(files)!=1:
            raise RestoreError('Expected exactly one Dropbox file in encrypted object')
        target=files[0]
        commit={'path':destination,'mode':'add','autorename':False,
                'strict_conflict':True,'mute':False}
        with target.open('rb') as stream:
            if target.stat().st_size<=DIRECT_UPLOAD_LIMIT:
                return _dropbox_upload('upload',token,commit,stream.read())
            # Dropbox upload sessions provide a bounded-memory restore of
            # arbitrarily large encrypted backup objects.
            chunk=8*1024*1024
            first=stream.read(chunk)
            session=_dropbox_upload('upload_session/start',token,
                                    {'close':False},first,required='session_id')
            session_id=session['session_id']
            position=len(first)
            total=target.stat().st_size
            while total-position>chunk:
                next_block=stream.read(chunk)
                if not next_block:
                    raise RestoreError('Local backup file ended during upload session')
                _dropbox_upload('upload_session/append_v2',token,
                    {'cursor':{'session_id':session_id,'offset':position},
                     'close':False},next_block,required=None)
                position+=len(next_block)
            remainder=stream.read()
            if position+len(remainder)!=total:
                raise RestoreError('Local backup file changed during restore')
            return _dropbox_upload('upload_session/finish',token,
                {'cursor':{'session_id':session_id,'offset':position},
                 'commit':commit},remainder)
