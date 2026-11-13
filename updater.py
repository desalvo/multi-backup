"""Read-only GitHub Releases update discovery and hash-verified staging.

For safety, no installer is run silently. GUI and CLI ask the user to approve
opening/installing a verified package; OS-signature checks remain mandatory
before production-ready unattended installation.
"""
from __future__ import annotations
import hashlib
import json
import os
import platform
import re
import ssl
import tempfile
import urllib.parse
import urllib.request
from pathlib import Path
from version import VERSION, REPOSITORY
import certifi

API = 'https://api.github.com/repos/'+REPOSITORY+'/releases'


def _verified_https_context():
    # Explicit Mozilla CA roots work inside frozen apps where OpenSSL's compiled
    # system trust path may not exist. Never use an unverified context.
    return ssl.create_default_context(cafile=certifi.where())
ASSET_LIMIT=2*1024*1024*1024


def release_endpoint(channel):
    if channel=='stable':return API+'/latest'
    if channel=='early-release':return API+'/tags/early-release'
    raise ValueError('Invalid update channel')


def detect_asset_prefix(os_name=None, machine=None, distro=None, distro_version=None):
    os_name=(os_name or platform.system()).lower();machine=(machine or platform.machine()).lower()
    if os_name=='darwin':
        if machine not in ('arm64','aarch64','x86_64','amd64'):
            raise RuntimeError('Unsupported macOS architecture')
        return 'multi-backup-macos-'+('arm64' if machine in ('arm64','aarch64') else 'x86_64')
    if os_name=='windows':
        if machine not in ('amd64','x86_64'):
            raise RuntimeError('Unsupported Windows architecture')
        return 'multi-backup-windows11-x64'
    if os_name=='linux':
        if machine not in ('x86_64','amd64'):
            raise RuntimeError('Unsupported Linux architecture')
        if distro is None or distro_version is None:
            try:
                values={}
                for line in Path('/etc/os-release').read_text('utf-8').splitlines():
                    if '=' in line:
                        key,value=line.split('=',1)
                        values[key]=value.strip().strip('\"')
                distro=distro or values.get('ID','')
                distro_version=distro_version or values.get('VERSION_ID','')
            except OSError as exc:
                raise RuntimeError('Cannot identify Linux distribution') from exc
        name={'rocky':'rocky','almalinux':'alma','alma':'alma'}.get(str(distro).lower())
        major=str(distro_version).split('.',1)[0]
        if not name or major not in ('8','9','10'):
            raise RuntimeError('Native RPM update is available only for Rocky/AlmaLinux 8, 9, or 10')
        return f'multi-backup-{name}-el{major}-'
    raise RuntimeError('No update package for this OS')


def _get_json(url):
    # Fixed, HTTPS-only GitHub REST endpoints; not an arbitrary URL fetcher.
    if url not in (release_endpoint('stable'), release_endpoint('early-release')):
        raise ValueError('Untrusted GitHub Releases endpoint')
    req=urllib.request.Request(url,headers={'Accept':'application/vnd.github+json','User-Agent':'multi-backup/'+VERSION})
    with urllib.request.urlopen(req,timeout=20,context=_verified_https_context()) as response:  # nosec B310 - fixed HTTPS GitHub API endpoint
        return json.loads(response.read(2*1024*1024))


def get_release(channel):
    rel=_get_json(release_endpoint(channel))
    if not isinstance(rel,dict) or not isinstance(rel.get('assets'),list):
        raise RuntimeError('Unexpected release response')
    # Require explicit tag identity in the early channel.
    if channel=='early-release' and rel.get('tag_name')!='early-release':
        raise RuntimeError('Unexpected early release tag')
    if channel=='stable' and rel.get('prerelease'):
        raise RuntimeError('Stable release must not be a prerelease')
    return rel


def select_asset(release,os_name=None,machine=None):
    prefix=detect_asset_prefix(os_name,machine)
    assets=release.get('assets',[])
    # installers are preferred over raw archives where available.
    candidates=[]
    for obj in assets:
        name=obj.get('name','')
        if name.startswith(prefix) and name.lower().endswith(('.dmg','.exe','.rpm','.zip')):
            candidates.append(obj)
    candidates.sort(key=lambda a:({'.exe':0,'.dmg':0,'.rpm':0,'.zip':1}.get(Path(a['name']).suffix.lower(),2),a['name']))
    return candidates[0] if candidates else None


def update_info(channel,os_name=None,machine=None):
    rel=get_release(channel)
    asset=select_asset(rel,os_name,machine)
    return {'channel':channel,'installed':VERSION,'release':rel.get('name',rel.get('tag_name')),
            'tag':rel.get('tag_name'),'url':rel.get('html_url'),'published_at':rel.get('published_at'),
            'asset':asset,'available':bool(asset)}


def _allowed_asset_uri(uri):
    parsed=urllib.parse.urlparse(uri)
    return parsed.scheme=='https' and parsed.netloc.lower() in ('github.com','objects.githubusercontent.com','release-assets.githubusercontent.com')


def download_asset(asset, output_dir, expected_sha256):
    """Stage a release asset, requiring an independently supplied SHA256 hash."""
    if not re.fullmatch('[a-fA-F0-9]{64}',expected_sha256 or ''):
        raise ValueError('Expected SHA-256 digest must be supplied')
    if not isinstance(asset,dict) or not _allowed_asset_uri(asset.get('browser_download_url','')):
        raise ValueError('Untrusted asset URL')
    name=asset.get('name','')
    if name!=Path(name).name or '/' in name or '\\' in name:
        raise ValueError('Unsafe asset filename')
    folder=Path(output_dir).expanduser()
    if folder.is_symlink():raise ValueError('Output directory cannot be a symlink')
    folder.mkdir(parents=True,exist_ok=True,mode=0o700)
    dst=folder/name
    if dst.exists():
        if _sha(dst)==expected_sha256.lower():return dst
        raise FileExistsError('A different file exists with that name')
    fd,partial=tempfile.mkstemp(dir=folder,prefix='.multi-backup-update-',suffix='.partial')
    try:
        digest=hashlib.sha256();total=0
        with os.fdopen(fd,'wb') as out:
            req=urllib.request.Request(asset['browser_download_url'],headers={'User-Agent':'multi-backup/'+VERSION})
            with urllib.request.urlopen(req,timeout=90,context=_verified_https_context()) as inp:  # nosec B310 - HTTPS URL and redirects checked against allowlist
                if not _allowed_asset_uri(inp.geturl()):raise RuntimeError('Untrusted update redirect')
                for block in iter(lambda:inp.read(1024*1024),b''):
                    total+=len(block)
                    if total>ASSET_LIMIT:raise RuntimeError('Update exceeded download size limit')
                    digest.update(block);out.write(block)
                out.flush();os.fsync(out.fileno())
        if digest.hexdigest()!=expected_sha256.lower():
            raise RuntimeError('Update SHA-256 verification failed')
        os.replace(partial,dst)
        return dst
    finally:
        if os.path.exists(partial):os.unlink(partial)


def _sha(path):
    h=hashlib.sha256()
    with path.open('rb') as f:
        for data in iter(lambda:f.read(1024*1024),b''):h.update(data)
    return h.hexdigest()


def verified_asset_from_release(info):
    """Use GitHub's asset digest; do not auto-stage if digest unavailable."""
    asset=info.get('asset') or {}
    digest=asset.get('digest','')
    if isinstance(digest,str) and digest.startswith('sha256:') and re.fullmatch('[a-fA-F0-9]{64}',digest[7:]):
        return asset,digest[7:]
    raise RuntimeError('Release asset has no GitHub SHA-256 digest; staging blocked')
