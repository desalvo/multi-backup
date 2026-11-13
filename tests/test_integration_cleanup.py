"""End-to-end offline backup: check cleanup happens before streaming backup."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from test_streaming import FAKE_GH, FAKE_GIT, FAKE_AGE, SCRIPT

# An additional fake-gh dispatcher, placed ahead of the legacy fake API.
DISPATCH = r'''
if [[ "$1" == "api" ]]; then
  ep="${@: -1}"
  if [[ " $* " == *' --method DELETE '* ]]; then
    printf '%s\n' "$ep" >> "$FAKE_DELETE_LOG"
    exit 0
  fi
  data=''
  case "$ep" in
    /user/repos\?*)
      data='[{"full_name":"testuser/project","name":"project","owner":{"login":"testuser","type":"User"},"default_branch":"main","has_wiki":true}]' ;;
    /repos/testuser/project/actions/runs\?*)
      data='{"workflow_runs":[{"id":1,"head_branch":"main","event":"push","status":"completed","workflow_id":8,"created_at":"2026-11-01T00:00:00Z","run_attempt":1},{"id":2,"head_branch":"main","event":"push","status":"completed","workflow_id":8,"created_at":"2026-11-02T00:00:00Z","run_attempt":1}]}' ;;
    /repos/testuser/project/actions/artifacts\?*)
      data='{"artifacts":[{"id":101,"name":"coverage-report","expired":false,"workflow_run":{"id":1}},{"id":102,"name":"coverage-report","expired":false,"workflow_run":{"id":2}}]}' ;;
    /repos/testuser/project/actions/workflows\?*)
      data='{"workflows":[{"id":8,"path":".github/workflows/ci.yml"}]}' ;;
    /repos/testuser/project/contents/.github/workflows/ci.yml\?*)
      data='{"encoding":"base64","content":"bmFtZTogQ0kKb246IHB1c2gKam9iczoge30K"}' ;;
    /repos/testuser/project/tags\?*|/repos/testuser/project/releases\?*)
      data='[]' ;;
    /repos/testuser/project/actions/runs/1)
      data='{"id":1,"status":"completed"}' ;;
    /repos/testuser/project/actions/artifacts/101)
      data='{"id":101,"name":"coverage-report","expired":false,"workflow_run":{"id":1}}' ;;
    /repos/testuser/project/actions/runs/[12]/artifacts\?*)
      data='{"artifacts":[{"id":102,"name":"coverage-report","expired":false}]}' ;;
    /repos/testuser/project/actions/runs/[12]/jobs\?*)
      data='{"jobs":[]}' ;;
  esac
  if [[ -n "$data" ]]; then
    if [[ " $* " == *' --slurp '* ]]; then printf '[%s]\n' "$data"; else printf '%s\n' "$data"; fi
    exit 0
  fi
fi
'''

class IntegrationTests(unittest.TestCase):
    def test_cleanup_precedes_backup_and_obsolete_runs_not_rearchived(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            bindir = root / 'bin'; bindir.mkdir()
            gh = FAKE_GH.replace('set -eu\n', 'set -eu\n'+DISPATCH, 1)
            for tool, script in [('gh', gh), ('git', FAKE_GIT), ('age', FAKE_AGE)]:
                path = bindir / tool; path.write_text(script); path.chmod(0o700)
            env = os.environ.copy()
            env['PATH'] = str(bindir)+os.pathsep+env['PATH']
            env['FAKE_DELETE_LOG'] = str(root/'delete-log.txt')
            out = root/'output'
            command = [sys.executable, str(SCRIPT), '--output-dir', str(out), '--recipient', 'age1TEST',
                       '--cleanup-ci-artifacts', '--keep-ci-runs', '1', '--skip-gists', '--skip-release-assets']
            result = subprocess.run(command, text=True, capture_output=True, timeout=90, env=env)
            self.assertEqual(result.returncode, 0, result.stdout + '\n' + result.stderr)
            self.assertIn('/repos/testuser/project/actions/artifacts/101', (root/'delete-log.txt').read_text())
            archives = list((out/'_metadata').glob('*.tar.gz.age'))
            self.assertEqual(len(archives), 1)
            with tarfile.open(archives[0], 'r:gz') as tar:
                names = set(tar.getnames())
                self.assertIn('multi-backup/repositories/testuser/project/ci/runs/2/run.json', names)
                self.assertNotIn('multi-backup/repositories/testuser/project/ci/runs/1/run.json', names)
                index = json.load(tar.extractfile('multi-backup/repositories/testuser/project/ci/runs_ALL_RETAINED.json'))
                self.assertEqual([x['id'] for x in index], [2])
                manifest = json.load(tar.extractfile('multi-backup/backup_manifest.json'))
                report = manifest['ci_cleanup']['testuser/project']
                self.assertEqual(report['deleted_artifact_ids'], [101])
                self.assertEqual(report['keep_run_ids'], [2])
                self.assertEqual(manifest['counts']['mirrors_ok'], 1)

    def test_purge_only_after_successful_new_metadata_backup(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            bindir = root / 'bin'; bindir.mkdir()
            gh = FAKE_GH.replace('set -eu\n', 'set -eu\n'+DISPATCH, 1)
            for tool, script in [('gh', gh), ('git', FAKE_GIT), ('age', FAKE_AGE)]:
                path = bindir / tool; path.write_text(script); path.chmod(0o700)
            env = os.environ.copy()
            env['PATH'] = str(bindir)+os.pathsep+env['PATH']
            env['FAKE_DELETE_LOG'] = str(root/'delete-log.txt')
            out = root/'output'
            baseline = [sys.executable, str(SCRIPT), '--output-dir', str(out), '--recipient', 'age1TEST']
            p1 = subprocess.run(baseline, text=True, capture_output=True, timeout=90, env=env)
            self.assertEqual(p1.returncode, 0, p1.stdout + '\n' + p1.stderr)
            old = list((out/'_metadata').glob('*.tar.gz.age'))
            self.assertEqual(len(old), 1)
            p2 = subprocess.run(baseline + ['--cleanup-ci-artifacts', '--keep-ci-runs', '1',
                                           '--purge-obsolete-metadata'],
                                text=True, capture_output=True, timeout=90, env=env)
            self.assertEqual(p2.returncode, 0, p2.stdout + '\n' + p2.stderr)
            self.assertFalse(old[0].exists())
            self.assertFalse(old[0].with_suffix(old[0].suffix + '.sha256').exists())
            self.assertEqual(len(list((out/'_metadata').glob('*.tar.gz.age'))), 1)
            # The protected Git backup remains, independent of metadata cleanup.
            self.assertEqual(len(list((out/'repositories').rglob('*.tar.gz.age'))), 1)

if __name__ == '__main__':
    unittest.main()
