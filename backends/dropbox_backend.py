"""Dropbox plugin: read-only, encrypted incremental snapshots, no source deletion.

Only /files/list_folder and /files/download endpoints are used. Nothing in this
module calls the Dropbox delete, move, or write APIs. Objects are content-addressed
by (file id, revision), so repeated runs reuse previous encrypted objects.
"""
from __future__ import annotations
import datetime as dt
import hashlib
import json
import os
import re
import shutil
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any
from checkpoint import atomic_json, sha256
from backup_sessions import start_dropbox_session, finalize_dropbox_session
from github_engine import StreamingArchive

API = 'https://api.dropboxapi.com/2/files/'
CONTENT = 'https://content.dropboxapi.com/2/files/download'


class DropboxAPI:
    def __init__(self, token: str, timeout=60):
        if not token or len(token.strip()) < 12:
            raise RuntimeError('Dropbox access token is missing')
        self.token=token.strip();self.timeout=timeout

    def _request(self, url, headers, data=None):
        # All requests are limited to fixed HTTPS endpoints owned by Dropbox.
        # This rules out file:, ftp: and user-controlled fetch targets.
        if url not in (API + 'list_folder', API + 'list_folder/continue', CONTENT):
            raise ValueError('Untrusted Dropbox API endpoint')
        for attempt in range(4):
            request=urllib.request.Request(url,headers=headers,data=data,method='POST')
            try:
                return urllib.request.urlopen(request,timeout=self.timeout)  # nosec B310 - fixed HTTPS Dropbox endpoint
            except urllib.error.HTTPError as exc:
                if exc.code not in (429,500,502,503,504) or attempt==3:
                    raise RuntimeError(f'Dropbox API failed (HTTP {exc.code})') from None
                # Explicitly bounded retries avoid unbounded jobs.
                time.sleep(min(8,2**attempt))
        raise RuntimeError('Dropbox API retry limit reached')

    def json(self,endpoint: str, payload: dict):
        headers={'Authorization':'Bearer '+self.token,'Content-Type':'application/json'}
        with self._request(API+endpoint,headers,json.dumps(payload).encode('utf-8')) as res:
            return json.loads(res.read(20*1024*1024))

    def list_entries(self,path: str=''):
        data=self.json('list_folder',{'path':path,'recursive':True,'include_deleted':False,'limit':2000})
        while True:
            for item in data.get('entries',[]):
                if not isinstance(item,dict) or item.get('.tag') not in ('file','folder'):
                    continue
                yield item
            if not data.get('has_more'):return
            if not isinstance(data.get('cursor'),str):
                raise RuntimeError('Dropbox pagination cursor is missing')
            data=self.json('list_folder/continue',{'cursor':data['cursor']})

    def download(self,file_id: str, revision: str, dst: Path):
        if not file_id.startswith('id:'):
            file_id='id:'+file_id
        headers={'Authorization':'Bearer '+self.token,
                 'Dropbox-API-Arg':json.dumps({'path':file_id},separators=(',',':'))}
        with self._request(CONTENT,headers) as stream:
            meta=json.loads(stream.headers.get('Dropbox-API-Result','{}'))
            if not meta or meta.get('rev') != revision:
                raise RuntimeError('Dropbox file changed during download; retry on next cycle')
            with dst.open('xb') as out:
                shutil.copyfileobj(stream,out,length=1024*1024)
            return meta


def object_key(item):
    if item.get('.tag')!='file' or not item.get('id') or not item.get('rev'):
        raise ValueError('Invalid Dropbox revision metadata')
    return hashlib.sha256((item['id']+'\x00'+item['rev']).encode('utf-8')).hexdigest()


def dropbox_content_hash(source: Path) -> str:
    """Dropbox SHA-256 of concatenated 4-MiB block SHA-256 digests."""
    overall = hashlib.sha256()
    with source.open('rb') as stream:
        while True:
            block = stream.read(4 * 1024 * 1024)
            if not block:
                break
            overall.update(hashlib.sha256(block).digest())
    return overall.hexdigest()


def verify_downloaded_revision(source: Path, item: dict, metadata: dict) -> None:
    """Reject changed, truncated or corrupted data before encrypting it."""
    if not isinstance(metadata, dict) or metadata.get('rev') != item.get('rev'):
        raise RuntimeError('Dropbox revision changed during download')
    if metadata.get('id') != item.get('id'):
        raise RuntimeError('Dropbox file identity changed during download')
    expected_size = item.get('size')
    if type(expected_size) is not int or expected_size < 0:
        raise RuntimeError('Dropbox listing omitted a valid file size')
    if metadata.get('size') != expected_size or source.stat().st_size != expected_size:
        raise RuntimeError('Dropbox download length differs from listing metadata')
    expected_hash = item.get('content_hash')
    if not isinstance(expected_hash, str) or not re.fullmatch(r'[0-9a-f]{64}', expected_hash):
        raise RuntimeError('Dropbox listing omitted a valid content hash')
    downloaded_hash = metadata.get('content_hash')
    if downloaded_hash is not None and downloaded_hash != expected_hash:
        raise RuntimeError('Dropbox download content hash differs from listing')
    if dropbox_content_hash(source) != expected_hash:
        raise RuntimeError('Dropbox downloaded bytes failed content hash verification')


def token_from_environment():
    value=os.environ.get('MULTI_BACKUP_DROPBOX_TOKEN','').strip()
    if value:return value
    try:
        import keyring
        return keyring.get_password('multi-backup','dropbox') or ''
    except (ImportError,Exception):
        return ''


def run(args, progress=None, api=None):
    """Create immutable encrypted file shards and a versioned encrypted manifest.

    Files removed from Dropbox are *not* removed from backup storage. The user
    intentionally controls any local cleanup of snapshots outside this plugin.
    """
    if getattr(args,'cleanup_ci_artifacts',False) or getattr(args,'purge_obsolete_metadata',False):
        raise ValueError('Dropbox backend cannot be combined with cleanup/purge options')
    if getattr(args,'max_copies_per_repo',5) != 5:
        # Git-specific option has no meaning here; reject misleading requests.
        raise ValueError('Git-specific retention options are not applicable to Dropbox')
    if not args.recipient:
        raise ValueError('Dropbox incremental backups require --recipient age1...')
    if not shutil.which('age'):
        raise RuntimeError('age executable is required for encrypted backups')
    api=api or DropboxAPI(token_from_environment())
    output=Path(args.output_dir).expanduser().resolve()
    stage=Path(args.staging_dir or output).expanduser().resolve()
    for folder in (output,stage):
        if folder.is_symlink():raise ValueError('Storage paths must not be symlinks')
        folder.mkdir(parents=True,exist_ok=True,mode=0o700)
        if os.name!='nt' and folder.stat().st_mode & 0o077:
            raise PermissionError('Dropbox backup paths must be private (chmod 700)')
    root=output/'dropbox'
    objects=root/'objects'
    manifests=root/'manifests'
    objects.mkdir(parents=True,exist_ok=True,mode=0o700)
    manifests.mkdir(parents=True,exist_ok=True,mode=0o700)
    manifest_name,resumed=start_dropbox_session(
        output,args.recipient,getattr(args,'dropbox_path','') or '',
        resume=getattr(args,'resume',False),restart=getattr(args,'restart',False))
    name=manifests/manifest_name
    if resumed:
        message=(f'RIPRESA backup Dropbox: archivio {manifest_name} invariato; '
                 'oggetti già cifrati e verificati verranno riutilizzati.')
        if progress:progress.emit('resume-start',message)
        else:print(message,flush=True)
        if name.is_file():
            # Interrupted after the final durable manifest was published.
            finalize_dropbox_session(output,name)
            if progress:progress.emit('complete','Dropbox backup già completato: '+manifest_name)
            return 0
    elif progress:
        progress.emit('cycle-start','Nuovo backup Dropbox: '+manifest_name)
    counts={'files_seen':0,'saved':0,'reused':0,'folders':0,'errors':0}
    entries=[]
    for item in api.list_entries(getattr(args,'dropbox_path','') or ''):
        if item['.tag']=='folder':
            counts['folders']+=1
            entries.append({'kind':'folder','path':item.get('path_display','')})
            continue
        key=object_key(item)
        dest=objects/key[:2]/(key+'.tar.gz.age')
        sidecar=dest.with_suffix(dest.suffix+'.sha256')
        counts['files_seen']+=1
        reused=False
        if dest.exists() and sidecar.exists():
            parts=sidecar.read_text('utf-8').split()
            if parts and parts[0]==sha256(dest):
                counts['reused']+=1
                reused=True
        if dest.exists() and not reused:
            # Keep existing ciphertext for possible recovery, never overwrite silently.
            raise RuntimeError('Encrypted Dropbox object failed SHA-256 verification; preserved: '+str(dest))
        if not reused:
            dest.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
            pending=dest.with_name(dest.name+'.partial')
            pending.unlink(missing_ok=True)
            with tempfile.TemporaryDirectory(prefix='.dropbox-item-',dir=stage) as tmp:
                source=Path(tmp)/'payload'
                try:
                    downloaded = api.download(item['id'],item['rev'],source)
                    verify_downloaded_revision(source,item,downloaded)
                    with StreamingArchive(pending,args.recipient) as archive:
                        archive.add_tree(source,'multi-backup/dropbox/payload')
                    os.replace(pending,dest)
                    sha=sha256(dest)
                    sidecar.write_text(f'{sha}  {dest.name}\n',encoding='utf-8')
                    counts['saved']+=1
                finally:
                    pending.unlink(missing_ok=True)
        entries.append({'kind':'file','id':item['id'],'path':item.get('path_display',''),
                        'rev':item['rev'],'bytes':item.get('size',0),
                        'content_hash':item.get('content_hash'),
                        'encrypted_object':str(dest.relative_to(root))})
        if progress:
            progress.emit('dropbox',f"{item.get('name','item')}: {'reused' if reused else 'encrypted'}",
                          completed=counts['files_seen'],total=max(counts['files_seen']+1,1))
    now=manifest_name.removesuffix('.tar.gz.age')
    manifest={'schema':1,'backend':'dropbox','created_utc':now,'counts':counts,'entries':entries,
              'retention':'append-only; no remote delete/purge APIs'}
    temp=name.with_name(name.name+'.partial')
    try:
        with StreamingArchive(temp,args.recipient) as archive:
            archive.add_json('multi-backup/dropbox/manifest.json',manifest)
        os.replace(temp,name)
        name.with_suffix(name.suffix+'.sha256').write_text(f'{sha256(name)}  {name.name}\n',encoding='utf-8')
    finally:
        temp.unlink(missing_ok=True)
    finalize_dropbox_session(output,name)
    if progress:progress.emit('complete','Dropbox incremental backup completed: '+manifest_name,
                              summary=counts)
    print(json.dumps({'backend':'dropbox','manifest':str(name),'counts':counts},indent=2))
    return 0
