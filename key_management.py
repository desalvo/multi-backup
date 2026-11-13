"""Generate and inspect age identities without exposing private material in logs."""
from __future__ import annotations
import os
import re
import shutil
import subprocess
from pathlib import Path
from settings import settings_path

NAME = re.compile(r'^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$')

def private_key_path(name='default', root=None):
    if not NAME.fullmatch(name): raise ValueError('Invalid key name')
    return (root or settings_path().parent / 'keys') / (name+'.txt')

def generate_key(name='default',root=None):
    if not shutil.which('age-keygen'):
        raise RuntimeError('age-keygen is required; install age from the package manager')
    dest=private_key_path(name,root)
    if dest.is_symlink() or dest.exists():raise FileExistsError('Key already exists; refusing to overwrite')
    dest.parent.mkdir(parents=True, exist_ok=True,mode=0o700)
    os.chmod(dest.parent,0o700)
    proc=subprocess.run(['age-keygen','-o',str(dest)],text=True,capture_output=True,check=False)
    if proc.returncode:
        dest.unlink(missing_ok=True)
        raise RuntimeError('Age key generation failed')
    os.chmod(dest,0o600)
    return {'private_key_file':str(dest),'public_recipient':public_recipient(name,root)}

def public_recipient(name='default',root=None):
    path=private_key_path(name,root)
    return recipient_for_identity(path)


def recipient_for_identity(path):
    path=Path(path)
    if path.is_symlink() or not path.is_file():raise FileNotFoundError('Identity file not found')
    if os.name != 'nt' and path.stat().st_mode & 0o077:
        raise PermissionError('Private key permissions must be 0600')
    proc=subprocess.run(['age-keygen','-y',str(path)],text=True,capture_output=True,check=False)
    if proc.returncode or not proc.stdout.strip().startswith('age1'):
        raise RuntimeError('Invalid age identity')
    return proc.stdout.strip()


def ensure_default_identity(root=None):
    """Find or generate the private local age identity, never overwrite a key."""
    dest=private_key_path('default',root)
    if dest.parent.is_symlink() or dest.is_symlink():
        raise RuntimeError('Refusing a symlink in the local age identity path')
    if dest.exists():
        recipient=recipient_for_identity(dest)
        return {'private_key_file':str(dest),
                'public_recipient':recipient, 'created':False}
    data=generate_key('default',root)
    return dict(data,created=True)
