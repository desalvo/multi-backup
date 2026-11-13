"""Crash-safe, SHA-256-checked resume state and atomic output publication."""
from __future__ import annotations
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import threading


def sha256(path: Path) -> str:
    hash_ = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            hash_.update(chunk)
    return hash_.hexdigest()


def atomic_json(path: Path, data):
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    tmp = path.with_name(path.name + "." + secrets.token_hex(6) + ".partial")
    try:
        with tmp.open('x', encoding='utf-8') as fp:
            json.dump(data, fp, indent=2, ensure_ascii=False)
            fp.write('\n')
            fp.flush()
            os.fsync(fp.fileno())
        os.replace(tmp, path)
    finally:
        tmp.unlink(missing_ok=True)


class Journal:
    def __init__(self, directory: Path, configuration: dict, *, restart=False):
        self.directory = directory
        self.path = directory / 'active.json'
        self.lock = threading.RLock()
        directory.mkdir(parents=True, exist_ok=True, mode=0o700)
        digest = hashlib.sha256(json.dumps(configuration, sort_keys=True).encode()).hexdigest()
        if self.path.exists() and not restart:
            state = json.loads(self.path.read_text('utf-8'))
            if state.get('schema') != 1:
                raise RuntimeError('Unsupported checkpoint format: use --restart to create a new cycle')
            if state['config_hash'] != digest:
                raise RuntimeError('Resume configuration changed; use original flags or --restart')
            self.data = state
        else:
            name = dt.datetime.now(dt.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ') + '-' + secrets.token_hex(4)
            self.data = {'schema': 1, 'cycle': name, 'config_hash': digest,
                         'steps': {}, 'finished': False, 'started_utc': dt.datetime.now(dt.timezone.utc).isoformat()}
            atomic_json(self.path, self.data)
        if self.data.get('finished'):
            raise RuntimeError('Last cycle finished. Use --restart for a new cycle')

    @property
    def cycle(self):
        return self.data['cycle']

    def done(self, name: str):
        with self.lock:
            record = self.data['steps'].get(name)
            if not record:
                return False
            path = Path(record['path'])
            if (not path.is_file() or path.stat().st_size != record['size']
                    or sha256(path) != record['sha256']):
                return False
            return True

    def record(self, name: str, path: Path):
        record = {'path': str(path.absolute()), 'size': path.stat().st_size, 'sha256': sha256(path)}
        with self.lock:
            self.data['steps'][name] = record
            atomic_json(self.path, self.data)

    def finish(self):
        with self.lock:
            self.data['finished'] = True
            self.data['finished_utc'] = dt.datetime.now(dt.timezone.utc).isoformat()
            atomic_json(self.directory / ('cycle-' + self.cycle + '.json'), self.data)
            self.path.unlink(missing_ok=True)


def verify_file(path: Path, expected: str | None):
    return path.is_file() and isinstance(expected, str) and sha256(path) == expected
