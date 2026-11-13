#!/usr/bin/env python3
"""Policy checks run inside the single Production Gate job.

These checks are independent of local signing credentials; signing/notarization
is tested for real by the platform-specific native jobs, before publication.
"""
from __future__ import annotations
from pathlib import Path
import sys
import yaml

ROOT=Path(__file__).resolve().parents[1]

def load(path):
    # PyYAML uses YAML 1.1, where an `on:` key becomes boolean True.
    data=yaml.safe_load((ROOT/path).read_text('utf-8'))
    if isinstance(data, dict) and True in data:
        data['on'] = data.pop(True)
    if not isinstance(data,dict):
        raise RuntimeError(f'{path} is not a YAML mapping')
    return data


def validate():
    flow=load('.github/workflows/ci.yml')
    native=load('.github/workflows/build-native.yml')
    jobs=flow['jobs']; assert 'production-gate' in jobs
    assert 'native-release' in jobs
    assert jobs['macos-smoke']['needs']==['production-gate'], 'macOS smoke must follow the production gate'
    assert jobs['native-release']['needs']==['production-gate','macos-smoke'], 'Native release must wait for gate and app startup'
    assert jobs['native-release']['uses']=='./.github/workflows/build-native.yml'
    assert 'workflow_call' in native['on'] and len(native['on'])==1, 'Native workflow must not publish independently'
    release=native['jobs']
    for name in ('macos','windows','linux-rpm'):
        assert 'qualify' in release[name]['needs'],f'{name} skipped release qualification'
    assert {'qualify','macos','windows','linux-rpm'}.issubset(set(release['publish']['needs']))
    assert {'qualify','macos','windows','linux-rpm'}.issubset(set(release['rpm-repositories']['needs']))
    assert 'if' in release['rpm-repositories'] and 'early-release' in release['rpm-repositories']['if']
    gate_steps='\n'.join(str(x.get('run','')) for x in jobs['production-gate']['steps'])
    for token in ('ruff check','bandit','pip_audit -r requirements.txt','pip_audit -r requirements-docs.txt',
                  'python -m unittest discover','check-production-gate.py'):
        assert token in gate_steps,f'Production Gate missing {token}'
    build_macos=(ROOT/'scripts/build-macos-app.sh').read_text('utf-8')
    notarize=(ROOT/'scripts/notarize-macos.sh').read_text('utf-8')
    verify=(ROOT/'scripts/verify-macos-dmg.sh').read_text('utf-8')
    assert "--windowed --onedir" in build_macos, 'Missing relocatable GUI bundle'
    assert "Contents/Resources/bin" in build_macos, 'Native helper executables not embedded'
    assert 'MAC_UNSIGNED_SMOKE' in build_macos, 'Missing unsigned app test mode'
    assert "--self-test" in build_macos, 'GUI smoke test missing'
    assert 'codesign --verify --deep --strict' in build_macos
    assert 'notarytool submit' in notarize and 'stapler staple' in notarize
    assert 'ditto "$APP" "$TMP/dmg-root/Multi Backup.app"' in notarize
    assert 'spctl --assess' in verify and 'hdiutil attach' in verify
    assert 'MAC_UNSIGNED_SMOKE' in str(jobs['macos-smoke']['steps']), 'Missing unsigned app boot test'
    assert 'upload-artifact' not in str(jobs['macos-smoke']['steps']), 'Unsigned app must never be uploaded'
    native_macos_steps='\n'.join(x.get('name','') for x in release['macos']['steps'])
    for required in ('Developer ID','Notarize app','Mount DMG'):
        assert required in native_macos_steps
    for name in ('LICENSE','README.md','README.IT.md','docs/SECURITY.md',
                 'docs/manuals/Multi-Backup-Manuale-Utente-IT.pdf',
                 'docs/manuals/Multi-Backup-User-Manual-EN.pdf',
                 'docs/brochures/Multi-Backup-Brochure-IT.pdf',
                 'docs/brochures/Multi-Backup-Brochure-EN.pdf'):
        assert (ROOT/name).is_file() and (ROOT/name).stat().st_size,f'Missing release file: {name}'
    print('Production Gate policy: PASS')

if __name__=='__main__':
    try:validate()
    except (KeyError, ValueError, AssertionError, OSError) as ex:
        print(f'Production Gate policy: FAIL ({ex})',file=sys.stderr)
        sys.exit(1)
