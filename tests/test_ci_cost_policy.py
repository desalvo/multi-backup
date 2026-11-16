"""Static regression checks: costly CI jobs must not trigger on unrelated PR commits."""
from pathlib import Path
import unittest
ROOT=Path(__file__).resolve().parents[1]/'.github'/'workflows'
class CostPolicyTests(unittest.TestCase):
    def read(self,name): return (ROOT/name).read_text(encoding='utf-8')
    def test_full_ci_opt_in_and_required_minio(self):
        s=self.read('ci.yml')
        self.assertIn('types: [ready_for_review, labeled]',s)
        self.assertIn("github.event.label.name == 'full-ci'",s)
        self.assertIn('s3-vault-minio-real:',s)
        self.assertIn('native-release:',s)
    def test_heavy_s3_is_manual_or_labeled(self):
        for wf,label in [('ceph-rgw-s3-real-r049.yml','run-ceph-ci'),('s3-minio-live-r048.yml','run-minio-ci')]:
            with self.subTest(wf=wf):
                s=self.read(wf)
                self.assertIn('types: [labeled]',s)
                self.assertIn(label,s)
                self.assertNotIn("- 'rust/**'",s)
    def test_provider_integrations_are_scoped(self):
        files=['database-galera-integration.yml','database-live-integration.yml','database-pitr-integration.yml','oracle-free-live.yml','network-filesystems-live-r042.yml','posix-real-integration.yml','ssh-sshfs-live-integration.yml','webdav-live-r046.yml','lustre-oracle-guards.yml','s3-rustfs-live-r050.yml']
        for wf in files:
            with self.subTest(wf=wf):
                s=self.read(wf)
                self.assertNotIn("- 'rust/**'",s)
                self.assertNotIn('      - "rust/**"',s)
                self.assertIn('workflow_dispatch:',s)
                self.assertNotIn('rust/src/bin/multi-backup-native.rs',s)
                self.assertIn('  push:',s)
                self.assertNotIn("      - '.github/workflows/"+wf+"'",s)
                if wf == 's3-rustfs-live-r050.yml':
                    self.assertNotIn("      - 'rust/src/lib.rs'",s)
    def test_fast_component_selection(self):
        s=self.read('rust-focused.yml')
        self.assertIn('dorny/paths-filter@v3',s)
        self.assertIn('  push:',s)
        self.assertIn('github.event.before',s)
        self.assertNotIn('  pull_request:',s)
        self.assertIn("needs.changes.outputs.core == 'true'",s)
        self.assertIn("needs.changes.outputs.server == 'true'",s)
        self.assertIn("needs.changes.outputs.desktop == 'true'",s)
        self.assertIn('  push:',self.read('agent-cluster-live-pg.yml'))
if __name__=='__main__': unittest.main()
