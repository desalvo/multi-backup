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
    # r039 native agent builds are release-critical across all supported OSes.
    for agent_job in ('rust-agent-macos','rust-agent-windows',
                      'rust-agent-debian','rust-agent-rpm'):
        assert agent_job in jobs, 'Missing native agent build matrix: '+agent_job
        assert 'production-gate' in jobs[agent_job].get('needs',[]), (
            'Agent installation must remain gated by security: '+agent_job)
    agent_macos=str(jobs['rust-agent-macos'])
    agent_windows=str(jobs['rust-agent-windows'])
    agent_debian=str(jobs['rust-agent-debian'])
    agent_rpm=str(jobs['rust-agent-rpm'])
    assert all(v in agent_macos for v in ('arm64','x86_64','package-rust-agent-unix.sh'))
    assert all(v in agent_windows for v in ('x64','package-rust-agent-windows.ps1'))
    assert all(v in agent_debian for v in ('12','13','amd64','arm64'))
    assert all(v in agent_rpm for v in ('rocky','alma','8','9','10'))
    gate_steps=str(jobs['production-gate']['steps'])
    for evidence in ('agent_protocol','afs_volume','execution_policy',
                     'cluster_placement','check-documentation-consistency.py'):
        assert evidence in gate_steps, 'Agent/cluster Production Gate missing '+evidence
    for source in ('rust/src/bin/multi-backup-agent.rs',
                   'rust/src/afs_volume.rs',
                   'rust/src/agent_protocol.rs',
                   'rust/src/execution_policy.rs',
                   'rust/src/cluster_placement.rs',
                   'rust-server/src/cluster.rs',
                   'rust-server/src/cluster_worker.rs',
                   'rust-server/src/agent_registry.rs',
                   'rust-server/src/agent_api.rs',
                   'docs/AGENTS_AFS_R039_IT.md',
                   'docs/AGENTS_AFS_R039_EN.md',
                   'docs/CLUSTER_ORCHESTRATION_R039_IT.md',
                   'docs/CLUSTER_ORCHESTRATION_R039_EN.md'):
        assert (ROOT/source).is_file(), 'Missing native agent/cluster source or manual: '+source

    assert (ROOT/'scripts/check-confidential-artifacts.py').is_file()
    assert (ROOT/'docs/PRIVATE_ARCHITECTURE_WORKFLOW.md').is_file()
    assert 'native-release' in jobs
    assert (ROOT/'scripts/test-server-gateway-container.sh').is_file()
    assert 'test-server-gateway-container.sh' in str(jobs['rust-server-validation']['steps']), 'Missing runtime private-gateway security regression'
    assert jobs['macos-smoke']['needs']==['production-gate'], 'macOS smoke must follow the production gate'
    assert jobs['publications-smoke']['needs']==['production-gate'], 'GUI capture must follow the security gate'
    assert jobs['rust-desktop-smoke']['needs']==['production-gate'], 'Rust-only desktop must follow the security gate'
    assert jobs['rust-macos-native-preview']['needs']==['production-gate'], 'Rust-only macOS DMGs must follow production gate'
    assert jobs['rust-windows-native-preview']['needs']==['production-gate'], 'Rust-only Windows setup must follow production gate'
    assert jobs['rust-linux-rpm-native-preview']['needs']==['production-gate'], 'Rust-only EL RPMs must follow production gate'
    el_matrix=jobs['rust-linux-rpm-native-preview']['strategy']['matrix']['include']
    assert {(x['distro'],int(x['el'])) for x in el_matrix}=={
        ('rocky',8),('rocky',9),('rocky',10),('alma',8),('alma',9),('alma',10)
    }, 'Rust-only RPM builds must cover all six Rocky/Alma platforms'
    assert jobs['rust-linux-debian-native-preview']['needs']==['production-gate']
    deb_matrix=jobs['rust-linux-debian-native-preview']['strategy']['matrix']['include']
    assert {(int(x['version']),x['arch']) for x in deb_matrix}=={
        (12,'amd64'),(12,'arm64'),(13,'amd64'),(13,'arm64')
    }, 'Debian 12/13 AMD64/ARM64 matrix must be complete'
    assert 'package-rust-debian-preview.sh' in str(jobs['rust-linux-debian-native-preview']['steps'])
    assert (ROOT/'scripts/package-rust-debian-preview.sh').is_file()
    assert 'multi-backup-rust.spec' in str(jobs['rust-linux-rpm-native-preview']['steps']), 'Native Rust RPM spec missing'
    mac_matrix=jobs['rust-macos-native-preview']['strategy']['matrix']['include']
    assert {x['arch'] for x in mac_matrix}=={'arm64','x86_64'}, 'Both native Mac architectures are mandatory'
    assert 'package-rust-macos-preview.sh' in str(jobs['rust-macos-native-preview']['steps']), 'Native Mac DMG packaging missing'
    assert 'package-rust-windows-preview.ps1' in str(jobs['rust-windows-native-preview']['steps']), 'Native Windows installer packaging missing'
    assert 'multi-backup-rust-preview-ubuntu24-x64.tar.gz' in str(jobs['rust-desktop-smoke']['steps']), 'Native Linux preview archive missing'
    assert (ROOT/'packaging/windows/rust-native-preview.iss').is_file(), 'Native Windows installer definition missing'
    assert (ROOT/'scripts/package-rust-macos-preview.sh').is_file(), 'Native Mac packager missing'
    assert (ROOT/'scripts/package-rust-windows-preview.ps1').is_file(), 'Native Windows packager missing'
    assert 'swiftc -O packaging/macos/biometric.swift' in (ROOT/'scripts/package-rust-macos-preview.sh').read_text('utf-8'), 'Native Rust Mac bundle lacks Touch ID adapter'
    assert (ROOT/'rust/tests/plugin_dispatch_roundtrip.rs').is_file(), 'Native plugin functional test missing'
    assert (ROOT/'rust/tests/incremental_chain_roundtrip.rs').is_file(), 'Incremental lineage acceptance missing'
    assert (ROOT/'rust/tests/garbage_collection_roundtrip.rs').is_file(), 'Repack GC recovery acceptance missing'
    assert (ROOT/'scripts/check-documentation-consistency.py').is_file(), 'Rust r038 documentation gate missing'
    assert 'scripts/check-documentation-consistency.py' in str(jobs['production-gate']['steps']), 'Operator docs check must block publication'
    assert 'garbage_collection_roundtrip' in str(jobs['production-gate']['steps']), 'Automatic 8+2 repack acceptance must remain a Production Gate test'
    assert 'plugin_dispatch_roundtrip' in str(jobs['production-gate']['steps']), 'Native plugin functional recovery must remain a blocking gate'
    assert 'archive::extract_from_memory' in (ROOT/'rust-desktop/src/main.rs').read_text('utf-8'), 'Native GUI uses an unprotected plaintext restore path'
    assert 'rust-desktop/Cargo.toml' in str(jobs['rust-desktop-smoke']['steps'])
    assert '--smoke' in str(jobs['rust-desktop-smoke']['steps'])

    doc_steps='\\n'.join(str(step.get('run','')) for step in
                        jobs['publications-smoke']['steps'])
    assert 'scripts/capture-ui.py' in doc_steps and 'scripts/build-publications.py' in doc_steps
    assert 'docs/screenshots/about.png' in doc_steps, 'About screenshot validation missing'
    # Prevent releasing outdated PDF manuals while Rust operations change.
    for path in ('docs/R038_OPERATIONS_IT.md', 'docs/R038_OPERATIONS_EN.md'):
        guide=(ROOT/path).read_text('utf-8')
        for token in ('gc-preview','gc-collect','verify-chain',
                      'delete-backup','--mode incremental','--target-account',
                      'schedule run'):
            assert token in guide, f'Operational reference missing {token}: {path}'
    publisher=(ROOT/'scripts/build-publications.py').read_text('utf-8')
    for token in ('append_r038_rust_preview_chapters','Rust r038:',
                  'gc-collect','verify-chain'):
        assert token in publisher, f'PDF manual generator missing Rust r038: {token}'
    assert "for manual in docs/manuals/*.pdf" in doc_steps, 'PDF r038 content smoke missing'
    assert jobs['native-release']['needs']==[
        'production-gate','macos-smoke','publications-smoke',
        'rust-server-validation','rust-desktop-smoke',
        'rust-macos-native-preview','rust-windows-native-preview',
        'rust-linux-rpm-native-preview','rust-linux-debian-native-preview'
    ], 'Native release cannot bypass Rust-only cross-platform acceptance'
    assert jobs['publish-container']['needs']==[
        'production-gate','rust-server-validation','rust-desktop-smoke',
        'rust-macos-native-preview','rust-windows-native-preview',
        'rust-linux-rpm-native-preview','rust-linux-debian-native-preview','publications-smoke'
    ], 'Docker Hub publication cannot bypass Rust-only cross-platform acceptance'
    assert 'ENABLE_STABLE_RUST' in jobs['native-release']['if']
    assert 'ENABLE_STABLE_RUST' in jobs['publish-container']['if']
    assert jobs['native-release']['uses']=='./.github/workflows/build-native.yml'
    assert 'workflow_call' in native['on'] and len(native['on'])==1, 'Native workflow must not publish independently'
    release=native['jobs']
    for name in ('macos','windows','linux-rpm','linux-deb'):
        assert 'qualify' in release[name]['needs'],f'{name} skipped release qualification'
    assert {'qualify','macos','windows','linux-rpm','linux-deb'}.issubset(set(release['publish']['needs']))
    assert {'qualify','macos','windows','linux-rpm'}.issubset(set(release['rpm-repositories']['needs']))
    release_deb_matrix=release['linux-deb']['strategy']['matrix']['include']
    assert {(int(x['version']),x['arch']) for x in release_deb_matrix}=={
        (12,'amd64'),(12,'arm64'),(13,'amd64'),(13,'arm64')
    }, 'Release must package every supported Debian target'
    assert '.deb' in str(release['publish']['steps']), 'DEB release upload collection missing'
    assert 'if' in release['rpm-repositories'] and 'early-release' in release['rpm-repositories']['if']
    gate_steps='\n'.join(str(x.get('run','')) for x in jobs['production-gate']['steps'])
    for token in ('ruff check','bandit','pip_audit -r requirements.txt','pip_audit -r requirements-docs.txt',
                  'python -m unittest discover','check-production-gate.py','check-confidential-artifacts.py'):
        assert token in gate_steps,f'Production Gate missing {token}'
    build_macos=(ROOT/'scripts/build-macos-app.sh').read_text('utf-8')
    notarize=(ROOT/'scripts/notarize-macos.sh').read_text('utf-8')
    verify=(ROOT/'scripts/verify-macos-dmg.sh').read_text('utf-8')
    assert "--windowed --onedir" in build_macos, 'Missing relocatable GUI bundle'
    assert "Contents/Resources/bin" in build_macos, 'Native helper executables not embedded'
    assert 'MAC_UNSIGNED_SMOKE' in build_macos, 'Missing test app build mode'
    assert '--add-binary rust/target/release/libmulti_backup_core.dylib:rust-native' in build_macos, 'Mac CLI/GUI must embed the compiled Rust core'
    windows_steps=str(release['windows']['steps'])
    assert 'rust/target/release/multi_backup_core.dll;rust-native' in windows_steps, 'Windows CLI/GUI must embed the compiled Rust core'
    test_package=(ROOT/'scripts/package-macos-test-dmg.sh').read_text('utf-8')
    for token in ('Signature=adhoc','hdiutil create','hdiutil attach','ditto',
                  '--self-test','codesign --verify --deep --strict','checksums-macos-'):
        assert token in test_package, 'Missing TEST DMG invariant: '+token
    mac_steps=release['macos']['steps']
    for prefix in ('Import Developer ID','Notarize app','Mount DMG'):
        step=next(s for s in mac_steps if s.get('name','').startswith(prefix))
        assert step.get('if') == "needs.qualify.outputs.channel == 'stable'", 'Stable macOS trust requirement bypassed'
    test_step=next(s for s in mac_steps if s.get('name','').startswith('Create and verify early-release TEST DMG'))
    assert test_step.get('if') == "needs.qualify.outputs.channel == 'early-release'"
    assert 'MAC_UNSIGNED_SMOKE' in str(mac_steps), 'Missing early Mac test configuration'
    assert "--self-test" in build_macos, 'GUI smoke test missing'
    assert 'codesign --verify --deep --strict' in build_macos
    assert 'notarytool submit' in notarize and 'stapler staple' in notarize
    assert 'ditto "$APP" "$TMP/dmg-root/Multi Backup.app"' in notarize
    assert 'spctl --assess' in verify and 'hdiutil attach' in verify
    assert 'MAC_UNSIGNED_SMOKE' in str(jobs['macos-smoke']['steps']), 'Missing unsigned app boot test'
    smoke=str(jobs['macos-smoke']['steps'])
    assert 'macos-' in smoke and '-preview' in smoke, 'Missing clearly labeled macOS preview artifact'
    assert 'package-macos-test-dmg.sh' in smoke, 'Missing installable macOS preview DMG'
    assert 'TEST-ONLY' not in smoke, 'Legacy test-only image naming leaked'
    assert 'multi-backup-macos-' in test_package and '-test.dmg' not in test_package
    assert 'multi-backup-macos-arm64-test.dmg' in native['jobs']['publish']['steps'][-2].get('run','') or (
        'multi-backup-macos-arm64-test.dmg' in str(native['jobs']['publish']['steps'])), 'Old early images must be retired'
    assert 'gh release' not in smoke, 'Smoke job must never publish a GitHub release'
    native_macos_steps='\n'.join(x.get('name','') for x in release['macos']['steps'])
    for required in ('Developer ID','Notarize app','Mount DMG'):
        assert required in native_macos_steps
    for name in ('LICENSE','README.md','README.IT.md','docs/SECURITY.md',
                 'docs/manuals/Multi-Backup-Manuale-Utente-IT.pdf',
                 'docs/manuals/Multi-Backup-User-Manual-EN.pdf',
                 'docs/brochures/Multi-Backup-Brochure-IT.pdf',
                 'docs/brochures/Multi-Backup-Brochure-EN.pdf'):
        assert (ROOT/name).is_file() and (ROOT/name).stat().st_size,f'Missing release file: {name}'
    requirements=(ROOT/'requirements.txt').read_text('utf-8')
    assert 'cryptography>=50.0.0' in requirements, 'Unsupported vulnerable encryption dependency'
    assert 'certifi>=' in requirements, 'Portable CA trust roots must be shipped'
    vault=(ROOT/'secure_vault.py').read_text('utf-8')
    for token in ('hashlib.scrypt','AESGCM','migrate_legacy','export_recovery_copy',
                  'temporary_identity','biometry'):
        if token!='biometry':
            assert token in vault,'Protected vault missing '+token
    updater=(ROOT/'updater.py').read_text('utf-8')
    assert 'certifi.where()' in updater and 'ssl.create_default_context' in updater
    assert 'CERT_NONE' not in updater and 'unverified_context' not in updater
    restore=(ROOT/'restore_ops.py').read_text('utf-8')
    for token in ('validate_plugin','restore_github_new_repository','restore_dropbox_new_file',
                  'strict_conflict','upload_session/finish','RefuseRedirect'):
        assert token in restore,'Restore safety check missing '+token
    shell=(ROOT/'gui.py').read_text('utf-8')
    assert 'self.after(35,self._startup_security)' in shell
    assert 'Save encrypted recovery copy' in shell
    assert 'self._locked' in shell and "self.page_canvas" in shell
    assert 'Restore' in (ROOT/'sidebar_nav.py').read_text('utf-8')
    assert 'packaging/macos/biometric.swift' in build_macos
    native_helper=(ROOT/'packaging/macos/biometric.swift').read_text('utf-8')
    assert 'biometryCurrentSet' in native_helper and 'WhenUnlockedThisDeviceOnly' in native_helper
    assert '--gui-smoke' in (ROOT/'scripts/package-macos-test-dmg.sh').read_text('utf-8')
    print('Production Gate policy: PASS')

if __name__=='__main__':
    try:validate()
    except (KeyError, ValueError, AssertionError, OSError) as ex:
        import traceback
        traceback.print_exc()
        print(f'Production Gate policy: FAIL ({ex})',file=sys.stderr)
        sys.exit(1)
