"""Password-protected local age identity vault, independent of application updates.

Format v1: scrypt (N=32768,r=8,p=1) + AES-256-GCM authenticated envelope.
No password or unencrypted age private identity is persisted in preferences.
The vault and any recovery copy are self-contained and encrypted independently.
"""
from __future__ import annotations

import base64
import contextlib
import hashlib
import json
import os
import re
import secrets
import subprocess
import tempfile
from pathlib import Path

from cryptography.exceptions import InvalidTag
from cryptography.hazmat.primitives.ciphers.aead import AESGCM

from key_management import private_key_path
from settings import settings_path

SCHEMA=1
N=32768
MAX_VAULT_BYTES=65536
SECRET_RE=re.compile(rb'(?m)^AGE-SECRET-KEY-[A-Z0-9]+\s*$')
RECIPIENT_RE=re.compile(r'^age1[023456789acdefghjklmnpqrstuvwxyz]{20,}$')


class VaultError(RuntimeError):
    pass


def vault_path(root=None):
    directory=Path(root) if root is not None else settings_path().parent/'keys'
    return directory/'default.agevault'


def _encode(value):
    return base64.b64encode(value).decode('ascii')


def _decode(value):
    if not isinstance(value,str):
        raise VaultError('Invalid encoded vault field')
    try:
        return base64.b64decode(value,validate=True)
    except (ValueError,base64.binascii.Error) as exc:
        raise VaultError('Invalid base64 in protected vault') from exc


def _password_key(password, salt):
    if not isinstance(password,str) or not password:
        raise VaultError('Encryption password is required')
    if len(password.encode('utf-8'))>1024:
        raise VaultError('Password is too long')
    return hashlib.scrypt(password.encode('utf-8'),salt=salt,n=N,r=8,p=1,
                          dklen=32,maxmem=128*1024*1024)


def _aad(recipient):
    return ('multi-backup/identity-v1/'+recipient).encode('ascii')


def seal_identity(private_bytes, password, recipient):
    if not isinstance(private_bytes,bytes) or not SECRET_RE.search(private_bytes):
        raise VaultError('Invalid age private identity')
    if not isinstance(recipient,str) or not RECIPIENT_RE.fullmatch(recipient):
        raise VaultError('Invalid public recipient')
    if len(private_bytes)>16384:
        raise VaultError('Private identity exceeds size limit')
    salt=secrets.token_bytes(16)
    nonce=secrets.token_bytes(12)
    ciphertext=AESGCM(_password_key(password,salt)).encrypt(
        nonce,private_bytes,_aad(recipient))
    return {'schema':SCHEMA,'kdf':'scrypt','n':N,'r':8,'p':1,
            'cipher':'AES-256-GCM','recipient':recipient,
            'salt':_encode(salt),'nonce':_encode(nonce),
            'ciphertext':_encode(ciphertext)}


def open_identity(data, password):
    if not isinstance(data,dict) or data.get('schema')!=1 or (
            data.get('kdf'),data.get('n'),data.get('r'),data.get('p'),
            data.get('cipher'))!=('scrypt',N,8,1,'AES-256-GCM'):
        raise VaultError('Unsupported or malformed protected identity')
    recipient=data.get('recipient')
    if not isinstance(recipient,str) or not RECIPIENT_RE.fullmatch(recipient):
        raise VaultError('Invalid public recipient in vault')
    salt=_decode(data.get('salt'))
    nonce=_decode(data.get('nonce'))
    ciphertext=_decode(data.get('ciphertext'))
    if len(salt)!=16 or len(nonce)!=12 or not 16<len(ciphertext)<32768:
        raise VaultError('Invalid protected identity lengths')
    try:
        clear=AESGCM(_password_key(password,salt)).decrypt(
            nonce,ciphertext,_aad(recipient))
    except InvalidTag as exc:
        raise VaultError('Incorrect password or damaged encryption key vault') from exc
    if not SECRET_RE.search(clear):
        raise VaultError('Recovered age identity failed validation')
    return clear,recipient


def load_vault(path=None):
    path=Path(path) if path else vault_path()
    if path.is_symlink() or not path.is_file():
        raise VaultError('Protected encryption key not found')
    if path.stat().st_size>MAX_VAULT_BYTES:
        raise VaultError('Protected encryption key exceeds size limit')
    if os.name!='nt' and path.stat().st_mode & 0o077:
        raise VaultError('Protected vault file must have mode 0600')
    try:
        return json.loads(path.read_text('utf-8'))
    except (OSError,ValueError) as exc:
        raise VaultError('Cannot read protected age identity') from exc


def unlock(password, path=None):
    return open_identity(load_vault(path),password)


def _write_vault(path, data, *, overwrite=False):
    path=Path(path)
    if path.is_symlink() or path.parent.is_symlink():
        raise VaultError('Refusing a symlink in encrypted vault destination')
    if path.exists() and not overwrite:
        raise FileExistsError('Protected identity already exists; refusing overwrite')
    path.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
    if path.parent.is_symlink():
        raise VaultError('Refusing a symlinked key directory')
    if os.name!='nt':
        os.chmod(path.parent,0o700)
    raw=(json.dumps(data,sort_keys=True,separators=(',',':'))+'\n').encode('utf-8')
    fd,name=tempfile.mkstemp(prefix='.agevault-',suffix='.part',dir=path.parent)
    try:
        if os.name!='nt':
            os.fchmod(fd,0o600)
        with os.fdopen(fd,'wb') as stream:
            stream.write(raw);stream.flush();os.fsync(stream.fileno())
        if path.exists() and not overwrite:
            raise FileExistsError('Protected identity already exists')
        os.replace(name,path)
        if os.name!='nt':
            os.chmod(path,0o600)
    finally:
        if os.path.exists(name):os.unlink(name)


def create_identity(password, path=None):
    if len(password)<12:
        raise VaultError('Use a strong password of at least 12 characters')
    path=Path(path) if path else vault_path()
    legacy=private_key_path(root=path.parent)
    if path.exists() or legacy.exists() or path.is_symlink() or legacy.is_symlink():
        raise VaultError('An existing encryption identity must be unlocked or migrated')
    # age-keygen emits its private identity to stdout, not a disk file.
    proc=subprocess.run(['age-keygen'],capture_output=True,check=False,timeout=30)
    if proc.returncode or not SECRET_RE.search(proc.stdout):
        raise VaultError('Failed to create new age identity')
    # age-keygen -y reads private identity on stdin; avoids temporary plaintext.
    pub=subprocess.run(['age-keygen','-y'],input=proc.stdout,capture_output=True,
                       check=False,timeout=30)
    recipient=pub.stdout.decode('ascii','replace').strip()
    if pub.returncode or not RECIPIENT_RE.fullmatch(recipient):
        raise VaultError('Cannot derive the age public recipient')
    clear=proc.stdout
    data=seal_identity(clear,password,recipient)
    _write_vault(path,data)
    open_identity(load_vault(path),password)
    return clear,recipient


def migrate_legacy(password, path=None):
    """Convert previous plaintext default.txt into protected vault without changing keys."""
    path=Path(path) if path else vault_path()
    legacy=private_key_path(root=path.parent)
    if path.exists() or path.is_symlink():
        raise VaultError('Protected key already exists; migration refused')
    if legacy.is_symlink() or not legacy.is_file():
        raise VaultError('Legacy private key not found')
    if os.name!='nt' and legacy.stat().st_mode & 0o077:
        raise VaultError('Legacy private identity must be restricted to 0600')
    if len(password)<12:
        raise VaultError('Use a password of at least 12 characters')
    clear=legacy.read_bytes()
    pub=subprocess.run(['age-keygen','-y',str(legacy)],capture_output=True,
                       check=False,timeout=30)
    recipient=pub.stdout.decode('ascii','replace').strip()
    if pub.returncode or not RECIPIENT_RE.fullmatch(recipient):
        raise VaultError('Cannot determine recipient of previous identity')
    _write_vault(path,seal_identity(clear,password,recipient))
    recovered,_=unlock(password,path)
    if recovered!=clear:
        raise VaultError('Vault migration verification failed; legacy key preserved')
    # Explicitly avoid leaving a second plaintext key. On flash storage a deleted
    # file might still be recoverable; enabling device encryption is recommended.
    legacy.unlink()
    return recovered,recipient


def export_recovery_copy(password,backup_password,destination,source=None):
    """Independently password-encrypted recovery copy; never writes plaintext."""
    if len(backup_password)<12:
        raise VaultError('Recovery-copy password must have at least 12 characters')
    clear,recipient=unlock(password,source)
    _write_vault(destination,seal_identity(clear,backup_password,recipient))
    recovered,_=unlock(backup_password,destination)
    if recovered!=clear:
        raise VaultError('Recovery copy verification failed')
    return Path(destination)


def change_password(password,new_password,path=None):
    if len(new_password)<12:
        raise VaultError('New password must have at least 12 characters')
    clear,recipient=unlock(password,path)
    target=Path(path) if path else vault_path()
    _write_vault(target,seal_identity(clear,new_password,recipient),overwrite=True)
    return recipient


def import_recovery_copy(source, backup_password, new_password, destination=None):
    """Import an encrypted recovery copy only if no local protected vault exists."""
    destination=Path(destination) if destination else vault_path()
    if destination.is_symlink() or destination.exists() or (
            private_key_path(root=destination.parent).exists()):
        raise VaultError('An existing encryption identity must never be overwritten')
    if len(new_password)<12:
        raise VaultError('The local password must have at least 12 characters')
    clear,recipient=unlock(backup_password,source)
    _write_vault(destination,seal_identity(clear,new_password,recipient))
    if unlock(new_password,destination)!=(clear,recipient):
        raise VaultError('Imported recovery key failed verification')
    return clear,recipient



def recover_existing_vault(recovery_path,recovery_password,new_password,
                           destination=None):
    """Recover access from a separate encrypted backup without discarding old vault.

    The existing vault is renamed to a nonoverwritten .previous file. If its
    metadata is readable, recipient matching prevents replacing the wrong key.
    """
    target=Path(destination) if destination else vault_path()
    if not target.exists() or target.is_symlink():
        raise VaultError('A protected local vault must exist for recovery')
    clear,recipient=unlock(recovery_password,recovery_path)
    if len(new_password)<12:
        raise VaultError('New password must contain at least 12 characters')
    try:
        previous=load_vault(target)
        known=previous.get('recipient')
        if known and known!=recipient:
            raise VaultError('Recovery copy belongs to a different age identity')
    except VaultError as exc:
        if 'different age identity' in str(exc):
            raise
        # Corrupted metadata can be recovered using a known-good encrypted copy.
    preserved=target.with_name(target.name+'.previous')
    if preserved.exists() or preserved.is_symlink():
        raise VaultError('A previous protected vault already exists; refusing overwrite')
    os.replace(target,preserved)
    try:
        _write_vault(target,seal_identity(clear,new_password,recipient))
        if unlock(new_password,target)!=(clear,recipient):
            raise VaultError('Restored vault did not decrypt correctly')
    except Exception:
        target.unlink(missing_ok=True)
        os.replace(preserved,target)
        raise
    return clear,recipient


@contextlib.contextmanager
def temporary_identity(private_bytes):
    """Private identity on disk only while an age subprocess needs a filename."""
    if not isinstance(private_bytes,bytes) or not SECRET_RE.search(private_bytes):
        raise VaultError('Unlocked age identity not available')
    base=settings_path().parent
    if base.is_symlink():
        raise VaultError('Invalid private application support directory')
    base.mkdir(parents=True,exist_ok=True,mode=0o700)
    directory=tempfile.mkdtemp(prefix='.age-use-',dir=base)
    target=Path(directory)/'identity.txt'
    try:
        fd=os.open(target,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
        with os.fdopen(fd,'wb') as stream:
            stream.write(private_bytes)
            stream.flush()
        yield target
    finally:
        target.unlink(missing_ok=True)
        Path(directory).rmdir()
