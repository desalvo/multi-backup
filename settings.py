"""Per-user settings; never persist access tokens or private keys here."""
from __future__ import annotations
import json
import os
import re
from pathlib import Path
from checkpoint import atomic_json

DEFAULTS = {
    'update_channel': 'stable',
    'auto_update_check': True,
    'update_interval_hours': 6,
    'enabled_backends': ['github'],
    'output_directory': str(Path.home() / 'Multi-Backups'),
    'metadata_workers': 4,
    'github_username': '',
    'backup_ci_runs': 5,
    'keep_ci_runs': 5,
    'cleanup_ci_artifacts': False,
    'github_cleanup_dry_run': True,
    'ignore_ci_artifact_errors': False,
    'biometric_enabled': False,
    'default_recipient': '',
}


def settings_path() -> Path:
    if os.name == 'nt':
        home = Path(os.environ.get('APPDATA', Path.home() / 'AppData/Roaming'))
    elif os.uname().sysname == 'Darwin':
        home = Path.home() / 'Library/Application Support'
    else:
        home = Path(os.environ.get('XDG_CONFIG_HOME', Path.home() / '.config'))
    return home / 'multi-backup' / 'settings.json'


def load_settings(path: Path | None = None) -> dict:
    path = path or settings_path()
    if path.is_symlink():
        raise RuntimeError('Refusing a symlink settings file')
    obj = {}
    if path.is_file():
        obj = json.loads(path.read_text('utf-8'))
        if not isinstance(obj, dict):
            raise ValueError('Invalid configuration format')
    result = dict(DEFAULTS)
    result.update({k: v for k,v in obj.items() if k in DEFAULTS})
    if type(result['biometric_enabled']) is not bool:
        raise ValueError('Invalid biometric setting')
    if type(result['ignore_ci_artifact_errors']) is not bool:
        raise ValueError('Invalid artifact error policy')
    if result['update_channel'] not in ('stable','early-release'):
        raise ValueError('Invalid update channel')
    if not isinstance(result['enabled_backends'], list) or not all(
            x in ('github','dropbox') for x in result['enabled_backends']):
        raise ValueError('Invalid backends')
    if not isinstance(result['update_interval_hours'], int) or not 1 <= result['update_interval_hours'] <= 168:
        raise ValueError('Invalid update interval')
    if not isinstance(result['github_username'],str) or (
            result['github_username'] and not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9-]{0,38}',result['github_username'])):
        raise ValueError('Invalid GitHub username in preferences')
    for field in ('backup_ci_runs','keep_ci_runs','metadata_workers'):
        if type(result[field]) is not int or not 1 <= result[field] <= (16 if field=='metadata_workers' else 1000):
            raise ValueError('Invalid backup/cleanup limit: '+field)
    return result


def save_settings(data: dict, path: Path | None = None) -> None:
    path = path or settings_path()
    if path.is_symlink() or path.parent.is_symlink():
        raise RuntimeError('Refusing symlink in settings path')
    safe = {k:data[k] for k in DEFAULTS if k in data}
    if any('token' in k.lower() or 'password' in k.lower() or 'secret' in k.lower() for k in safe):
        raise ValueError('Secrets must not be stored in settings')
    load_settings_check = dict(DEFAULTS, **safe)
    if type(load_settings_check['ignore_ci_artifact_errors']) is not bool:
        raise ValueError('Invalid artifact error policy')
    if load_settings_check['update_channel'] not in ('stable','early-release'):
        raise ValueError('Invalid update channel')
    if not isinstance(load_settings_check['github_username'],str) or (
            load_settings_check['github_username'] and
            not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9-]{0,38}',load_settings_check['github_username'])):
        raise ValueError('Invalid GitHub username')
    for field in ('backup_ci_runs','keep_ci_runs','metadata_workers'):
        value=load_settings_check[field]
        if type(value) is not int or not 1 <= value <= (16 if field=='metadata_workers' else 1000):
            raise ValueError('Invalid backup/cleanup limit: '+field)
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    os.chmod(path.parent,0o700)
    atomic_json(path,safe)
    os.chmod(path,0o600)


def set_backend_enabled(name: str, enabled: bool, path: Path | None = None):
    if name not in ('github','dropbox'):
        raise ValueError('Unknown backend')
    cfg = load_settings(path)
    selected = set(cfg['enabled_backends'])
    if enabled:selected.add(name)
    else:selected.discard(name)
    cfg['enabled_backends'] = [name for name in ('github','dropbox') if name in selected]
    save_settings(cfg,path)
    return cfg
