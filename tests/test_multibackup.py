"""Offline plugin, settings and update security tests."""
import argparse
import hashlib
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from backends import available
from backends.dropbox_backend import (object_key, DropboxAPI, run as dropbox_run,
                                      dropbox_content_hash, verify_downloaded_revision)
from settings import load_settings,save_settings,set_backend_enabled
from updater import release_endpoint,detect_asset_prefix,select_asset,verified_asset_from_release,download_asset
from key_management import private_key_path
from version import VERSION


class PluginTests(unittest.TestCase):
    def test_cli_global_help_includes_plugins_and_backup(self):
        import subprocess
        import sys
        completed=subprocess.run([sys.executable, str(Path(__file__).resolve().parents[1]/'multi_backup.py'), '--help'],
                                 capture_output=True,text=True,check=True)
        self.assertIn('plugins', completed.stdout)
        self.assertIn('backup', completed.stdout)
        self.assertIn('updates', completed.stdout)

    def test_rpm_has_graphical_executable(self):
        root=Path(__file__).resolve().parents[1]
        spec=(root/'packaging/linux/multi-backup.spec').read_text('utf-8')
        desktop=(root/'packaging/linux/multi-backup.desktop').read_text('utf-8')
        self.assertIn('Source4:        multi-backup-gui',spec)
        self.assertIn('%{_bindir}/multi-backup-gui',spec)
        self.assertIn('Exec=/usr/bin/multi-backup-gui',desktop)

    def test_registers_both_backends(self):
        backends=available()
        self.assertEqual(set(backends),{'github','dropbox'})
        self.assertFalse(backends['dropbox'].destructive_actions)
        self.assertTrue(backends['github'].default_enabled)
        self.assertFalse(backends['dropbox'].default_enabled)

    def test_plugin_is_off_by_default_and_can_be_toggled(self):
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp)/'settings.json'
            self.assertEqual(load_settings(path)['enabled_backends'],['github'])
            cfg=set_backend_enabled('dropbox',True,path)
            self.assertEqual(cfg['enabled_backends'],['github','dropbox'])
            set_backend_enabled('dropbox',False,path)
            self.assertEqual(load_settings(path)['enabled_backends'],['github'])

    def test_reject_invalid_channels_and_no_plaintext_secrets(self):
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp)/'settings.json'
            with self.assertRaises(ValueError):save_settings({'update_channel':'nightly'},path)
            with self.assertRaises(ValueError):set_backend_enabled('arbitrary',True,path)
            self.assertFalse(path.exists())

    def test_sane_public_version(self):
        self.assertRegex(VERSION,r'^\d+\.\d+\.\d+-r\d+-rc\d+$')

    def test_sidebar_navigation_uses_explicit_high_contrast_canvas_text(self):
        from sidebar_nav import (NAV_ITEMS,NAV_TEXT,NAV_NORMAL,NAV_HOVER,
                                 NAV_ACTIVE,SidebarNavItem,contrast_ratio)
        self.assertEqual([name for name,_ in NAV_ITEMS],
                         ['Dashboard','Backup','Archives','Plugins',
                          'Encryption','Updates','Settings'])
        for background in (NAV_NORMAL,NAV_HOVER,NAV_ACTIVE):
            self.assertGreaterEqual(contrast_ratio(NAV_TEXT,background),7.0)
        self.assertTrue(issubclass(SidebarNavItem,__import__('tkinter').Canvas))
        root=Path(__file__).resolve().parents[1]
        gui=(root/'gui.py').read_text('utf-8')
        sidebar=(root/'sidebar_nav.py').read_text('utf-8')
        self.assertIn('SidebarNavItem(self.sidebar,name,icon',gui)
        self.assertIn('self.nav[key].set_active(key==name)',gui)
        self.assertIn("fill=NAV_TEXT",sidebar)
        self.assertIn("self.bind('<Return>'",sidebar)
        self.assertIn("self.bind('<space>'",sidebar)
        self.assertIn("self.bind('<Button-1>'",sidebar)

    def test_dropbox_keys_are_stable_and_revision_sensitive(self):
        a={'.tag':'file','id':'id:aaa','rev':'ab12'}
        self.assertEqual(object_key(a),object_key(a))
        self.assertNotEqual(object_key(a),object_key(dict(a,rev='ab13')))
        with self.assertRaises(ValueError):object_key({'id':'id:a'})

    def test_dropbox_content_hash_vectors(self):
        with tempfile.TemporaryDirectory() as tmp:
            source=Path(tmp)/'file'
            source.write_bytes(b'')
            self.assertEqual(dropbox_content_hash(source),hashlib.sha256(b'').hexdigest())
            first=b'A'*(4*1024*1024)
            source.write_bytes(first+b'B')
            expected=hashlib.sha256(hashlib.sha256(first).digest()+
                                    hashlib.sha256(b'B').digest()).hexdigest()
            self.assertEqual(dropbox_content_hash(source),expected)
            source.write_bytes(b'example')
            self.assertEqual(dropbox_content_hash(source),
                             'f0a7447cc7c8ab136c4c253e224377ac108af790d55cd9a9dd372bf2a7a3e737')

    def test_dropbox_corrupted_revision_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            source=Path(tmp)/'payload'
            source.write_bytes(b'example')
            item={'.tag':'file','id':'id:abc','rev':'rev1','size':7,
                  'content_hash':dropbox_content_hash(source)}
            meta={key:item[key] for key in ('id','rev','size','content_hash')}
            verify_downloaded_revision(source,item,meta)
            for bad in (dict(meta,rev='other'),dict(meta,id='id:other'),
                        dict(meta,size=6),dict(meta,content_hash='0'*64)):
                with self.assertRaises(RuntimeError):
                    verify_downloaded_revision(source,item,bad)
            source.write_bytes(b'EXAMPLE')
            with self.assertRaises(RuntimeError):
                verify_downloaded_revision(source,item,meta)
            source.write_bytes(b'short')
            with self.assertRaises(RuntimeError):
                verify_downloaded_revision(source,item,meta)

    def test_dropbox_corrupted_download_does_not_publish(self):
        class CorruptAPI:
            def list_entries(self,path):
                yield {'.tag':'file','id':'id:abc','rev':'rev1','size':7,
                       'content_hash':'0'*64,'name':'example'}
            def download(self,file_id,revision,dst):
                dst.write_bytes(b'example')
                return {'id':file_id,'rev':revision,'size':7}
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)/'backup'
            args=argparse.Namespace(cleanup_ci_artifacts=False,purge_obsolete_metadata=False,
                    max_copies_per_repo=5,recipient='age1test',output_dir=root,staging_dir=None,
                    dropbox_path='')
            with mock.patch('backends.dropbox_backend.shutil.which',return_value='/usr/bin/age'):
                with self.assertRaises(RuntimeError):
                    dropbox_run(args,api=CorruptAPI())
            self.assertFalse(list(root.rglob('*.age')))
            self.assertFalse(list(root.rglob('*.partial')))

    def test_dropbox_rejects_remote_purge_options(self):
        args=argparse.Namespace(cleanup_ci_artifacts=True,purge_obsolete_metadata=False,
                                max_copies_per_repo=5,recipient='age1test')
        with self.assertRaises(ValueError):dropbox_run(args)

    def test_dropbox_requires_public_recipient(self):
        args=argparse.Namespace(cleanup_ci_artifacts=False,purge_obsolete_metadata=False,
                                max_copies_per_repo=5,recipient=None)
        with self.assertRaises(ValueError):dropbox_run(args)

    def test_release_channel_endpoints(self):
        self.assertTrue(release_endpoint('stable').endswith('/latest'))
        self.assertTrue(release_endpoint('early-release').endswith('/tags/early-release'))
        with self.assertRaises(ValueError):release_endpoint('nightly')

    def test_native_asset_prefixes(self):
        self.assertEqual(detect_asset_prefix('Darwin','arm64'),'multi-backup-macos-arm64')
        self.assertEqual(detect_asset_prefix('Darwin','x86_64'),'multi-backup-macos-x86_64')
        self.assertEqual(detect_asset_prefix('Windows','amd64'),'multi-backup-windows11-x64')
        self.assertEqual(detect_asset_prefix('Linux','x86_64','rocky','9.4'),'multi-backup-rocky-el9-')
        self.assertEqual(detect_asset_prefix('Linux','x86_64','almalinux','10'),'multi-backup-alma-el10-')
        with self.assertRaises(RuntimeError):detect_asset_prefix('Linux','x86_64','ubuntu','24.04')

    def test_package_filter_does_not_pick_unrelated_architecture(self):
        assets=[{'name':'multi-backup-macos-x86_64.dmg'},
                {'name':'multi-backup-macos-arm64.dmg'}]
        self.assertEqual(select_asset({'assets':assets},'Darwin','arm64')['name'],assets[1]['name'])

    def test_unverified_update_cannot_be_downloaded(self):
        info={'asset':{'name':'multi-backup-windows11-x64.exe','browser_download_url':'https://github.com/x/y'}}
        with self.assertRaises(RuntimeError):verified_asset_from_release(info)
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(ValueError):download_asset(info['asset'],tmp,'')
            with self.assertRaises(ValueError):download_asset(dict(info['asset'],browser_download_url='http://evil.example/file'),tmp,'a'*64)

    def test_security_workflow_enforces_correctness_checks_without_legacy_layout_gate(self):
        text=(Path(__file__).resolve().parents[1]/'.github/workflows/ci.yml').read_text('utf-8')
        self.assertIn('ruff check --select E4,E9,F63,F7,F82 --extend-select E722 .',text)
        self.assertIn('bandit -q -r .',text)
        self.assertIn('python -m pip_audit -r requirements.txt',text)

    def test_security_audit_dependency_policy_covers_all_native_builds(self):
        """Pillow <13 has 42 upstream advisories; never ship a vulnerable pinned binary."""
        root = Path(__file__).resolve().parents[1]
        requirements = (root/'requirements.txt').read_text('utf-8')
        documentation_requirements = (root/'requirements-docs.txt').read_text('utf-8')
        native = (root/'.github/workflows/build-native.yml').read_text('utf-8')
        security = (root/'.github/workflows/ci.yml').read_text('utf-8')
        self.assertIn('Pillow>=13.0.0,<14', requirements)
        self.assertIn('Pillow>=13.0.0,<14', documentation_requirements)
        self.assertEqual(native.count("'Pillow>=13.0.0,<14'"), 2)
        self.assertIn('python -m pip install -r requirements.txt', (root/'scripts/build-macos-app.sh').read_text('utf-8'))
        self.assertNotIn('Pillow>=10', native)
        self.assertNotIn('Pillow>=10', requirements)
        self.assertIn('python -m pip_audit -r requirements.txt', security)

    def test_single_production_gate_and_release_dependency(self):
        root=Path(__file__).resolve().parents[1]
        ci=(root/'.github/workflows/ci.yml').read_text('utf-8')
        native=(root/'.github/workflows/build-native.yml').read_text('utf-8')
        # Pure standard-library test: the full YAML parse runs later in the
        # security gate after PyYAML is installed, not in each Python matrix.
        self.assertIn('  production-gate:',ci)
        self.assertIn('  native-release:',ci)
        self.assertIn('    needs: [production-gate]',ci)
        self.assertIn('    needs: [production-gate, macos-smoke]',ci)
        self.assertIn('MAC_UNSIGNED_SMOKE',ci)
        self.assertIn('TEST-ONLY-macos-',ci)
        self.assertIn('package-macos-test-dmg.sh',ci)
        self.assertIn('    uses: ./.github/workflows/build-native.yml',ci)
        self.assertIn('on:\n  workflow_call:',native)
        self.assertNotIn('on:\n  push:',native)
        for step in ('pip_audit -r requirements.txt','pip_audit -r requirements-docs.txt',
                     'ruff check','bandit','unittest discover'):
            self.assertIn(step,ci)

    def test_mac_disk_image_contains_only_application_and_nothing_unsigned(self):
        root=Path(__file__).resolve().parents[1]
        build=(root/'scripts/build-macos-app.sh').read_text('utf-8')
        notarize=(root/'scripts/notarize-macos.sh').read_text('utf-8')
        verify=(root/'scripts/verify-macos-dmg.sh').read_text('utf-8')
        workflow=(root/'.github/workflows/build-native.yml').read_text('utf-8')
        self.assertIn('--windowed --onedir',build)
        self.assertIn('--osx-bundle-identifier',build)
        self.assertIn('Contents/MacOS/multi-backup',build)
        self.assertIn('Contents/Resources/bin',build)
        self.assertIn('MAC_UNSIGNED_SMOKE',build)
        self.assertIn('SIGN_ARGS',build)
        packager=(root/'scripts/package-macos-test-dmg.sh').read_text('utf-8')
        self.assertIn('Signature=adhoc',packager)
        self.assertIn('hdiutil attach',packager)
        self.assertIn('ditto "$MOUNT/Multi Backup.app"',packager)
        self.assertIn('checksums-macos-',packager)
        self.assertIn('codesign --verify --deep --strict',build)
        self.assertIn('notarytool submit',notarize)
        self.assertIn('stapler staple "$APP"',notarize)
        self.assertIn('stapler staple "$DMG"',notarize)
        self.assertIn('ditto "$APP" "$TMP/dmg-root/Multi Backup.app"',notarize)
        self.assertNotIn('cp dist/multi-backup-cli dmg-root',workflow)
        self.assertIn('hdiutil attach',verify)
        self.assertIn('spctl --assess',verify)
        self.assertIn('"$APP/Contents/MacOS/Multi Backup" --self-test',verify)
        self.assertIn('APPLE_SIGN_P12_B64',workflow)

    def test_gui_bundle_entrypoint_has_explicit_self_test(self):
        content=(Path(__file__).resolve().parents[1]/'gui_launcher.py').read_text('utf-8')
        self.assertIn('def bundle_self_test',content)
        self.assertIn("'--self-test'",content)

    def test_ci_workflow_uses_installable_windows_encryption_package(self):
        workflow=(Path(__file__).resolve().parents[1]/'.github/workflows/build-native.yml').read_text('utf-8')
        self.assertIn('choco install age.portable', workflow)
        self.assertNotIn('choco install age -y', workflow)
        self.assertIn('age.exe --version', workflow)

    def test_inno_setup_resolves_files_from_repository_root(self):
        root = Path(__file__).resolve().parents[1]
        text = (root/'packaging/windows/installer.iss').read_text('utf-8')
        self.assertIn('SourceDir=..\\..', text)
        self.assertIn('LicenseFile=LICENSE', text)
        self.assertIn('SetupIconFile=assets\\multi-backup.ico', text)
        self.assertIn('OutputDir=installers', text)
        self.assertIn('Source: "dist\\multi-backup.exe"', text)
        self.assertIn('Source: "dist\\MultiBackupGUI.exe"', text)

    def test_native_build_uses_real_age_binary_and_supported_rocky_images(self):
        workflow = (Path(__file__).resolve().parents[1]/'.github/workflows/build-native.yml').read_text('utf-8')
        self.assertIn("Join-Path $chocoRoot 'lib\\age.portable\\tools'", workflow)
        self.assertIn('Copy-Item -LiteralPath $real.FullName', workflow)
        self.assertIn('& dist/bin/age-keygen.exe --version', workflow)
        for release in (8,9,10):
            self.assertIn(f"image: 'rockylinux/rockylinux:{release}'", workflow)
            self.assertNotIn(f"image: 'rockylinux:{release}'", workflow)

    def test_dropbox_rejects_non_allowlisted_api_endpoints(self):
        from backends.dropbox_backend import DropboxAPI
        api=DropboxAPI('some-long-enough-token')
        for endpoint in ('file:///etc/passwd', 'https://evil.example/2/files/download'):
            with self.assertRaises(ValueError):
                api._request(endpoint, {})

    def test_age_key_path_sanitized(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(private_key_path('alpha',Path(tmp)).name,'alpha.txt')
            with self.assertRaises(ValueError):private_key_path('../danger',Path(tmp))

    def test_settings_file_not_world_readable(self):
        if os.name=='nt':self.skipTest('POSIX permissions test')
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp)/'cfg'/'settings.json'
            save_settings(load_settings(path),path)
            self.assertEqual(path.stat().st_mode&0o077,0)
            self.assertEqual(path.parent.stat().st_mode&0o077,0)

if __name__=='__main__':unittest.main()
