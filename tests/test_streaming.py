#!/usr/bin/env python3
"""Offline smoke test: fake gh/git/age; verifies streaming output, not real encryption."""
from pathlib import Path
import io
import ast
import runpy
from unittest import mock
import json
import os
import subprocess
import sys
import tarfile
import tempfile
import textwrap
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "github_disaster_backup.py"

FAKE_GH = r'''#!/usr/bin/env bash
set -eu
case "$1 $2" in
  "auth status") exit 0 ;;
  "auth token") echo SAMPLE_TOKEN_DO_NOT_USE; exit 0 ;;
  "run view") echo "Example CI log"; exit 0 ;;
  "run download")
    while [[ $# -gt 0 ]]; do if [[ "$1" == "--dir" ]]; then mkdir -p "$2"; printf 'Artifact content' > "$2/artifact.txt"; break; fi; shift; done
    exit 0 ;;
  "release download")
    while [[ $# -gt 0 ]]; do if [[ "$1" == "--dir" ]]; then mkdir -p "$2"; printf 'ZIP FAKE CONTENT' > "$2/release.zip"; break; fi; shift; done
    exit 0 ;;
esac
if [[ "$1" == "api" ]]; then
  ep="${@: -1}"
  case "$ep" in
    /user) data='{"login":"testuser"}' ;;
    /user/repos\?*) data='[{"full_name":"testuser/project","name":"project","owner":{"login":"testuser","type":"User"},"has_wiki":true}]' ;;
    */releases\?*) data='[{"id":1,"tag_name":"v1","assets":[{"id":7}]}]' ;;
    */actions/runs\?*) data='{"workflow_runs":[{"id":123,"run_attempt":2}]}' ;;
    */actions/runs/123/artifacts\?*) data='{"artifacts":[{"id":9,"expired":false}]}' ;;
    */actions/runs/123/*/jobs\?*|*/actions/runs/123/jobs\?*) data='{"jobs":[]}' ;;
    */actions/secrets\?*) data='{"secrets":[{"name":"MY_SECRET"}]}' ;;
    */secrets\?*|*/organization-secrets\?*) data='{"secrets":[]}' ;;
    */variables\?*|*/organization-variables\?*) data='{"variables":[]}' ;;
    */workflows\?*) data='{"workflows":[]}' ;;
    */runners\?*) data='{"runners":[]}' ;;
    */runner-groups\?*) data='{"runner_groups":[]}' ;;
    */environments\?*) data='{"environments":[{"id":3,"name":"production"}]}' ;;
    */deployment-branch-policies\?*) data='{"branch_policies":[]}' ;;
    /gists\?*|*/issues\?*|*/pulls\?*) data='[]' ;;
    *\?per_page=*|*/branches\?*|*/hooks\?*|*/rulesets\?*) data='[]' ;;
    *) data='{"ok":true}' ;;
  esac
  if [[ " $* " == *' --slurp '* ]]; then printf '[%s]\n' "$data"; else printf '%s\n' "$data"; fi
  exit 0
fi
printf 'Unknown gh call %s\n' "$*" >&2
exit 1
'''
FAKE_GIT = r'''#!/usr/bin/env bash
set -eu
if [[ "$1 $2" == "lfs version" ]]; then echo 'git-lfs/3.0';exit 0; fi
if [[ "$1 $2" == "clone --mirror" ]]; then
  dest="${@: -1}";mkdir -p "$dest/objects/pack";printf 'ref: refs/heads/main\n' > "$dest/HEAD"
  printf 'GIT PACK TEST' > "$dest/objects/pack/pack-test.pack";exit 0
fi
if [[ "$1" == "-C" ]]; then
  shift 2
  case "$1" in
    lfs|fsck) exit 0 ;;
    rev-parse) printf '%040x\n' 1; exit 0 ;;
    show) echo '2026-11-12T13:20:00+00:00'; exit 0 ;;
    for-each-ref)
      printf 'refs/heads/main %040x\n' 1
      if [[ -n "${BACKUP_FAKE_OTHER_REF:-}" ]]; then
        printf 'refs/heads/dev %040x\n' "${BACKUP_FAKE_OTHER_REF}"
      fi
      exit 0 ;;
    symbolic-ref) echo 'refs/heads/main'; exit 0 ;;
  esac
  exit 1
fi
exit 1
'''
FAKE_AGE = r'''#!/usr/bin/env bash
# Identity operation: NOT encryption, ONLY an offline streaming test.
set -eu
while [[ $# -gt 0 ]];do
  if [[ "$1" == "-o" ]];then cat > "$2";exit 0;fi
  shift
done
exit 1
'''

class Smoke(unittest.TestCase):
    def test_streaming_archive_structure(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); tools=root/'bin';tools.mkdir()
            for executable,code in [('gh',FAKE_GH),('git',FAKE_GIT),('age',FAKE_AGE)]:
                p=tools/executable;p.write_text(code);p.chmod(0o700)
            out=root/'output'; creds=root/'creds';creds.mkdir()
            (creds/'original_secret.txt').write_text('EXAMPLE_LOCAL_SECRET')
            env=os.environ.copy();env['PATH']=str(tools)+os.pathsep+env['PATH']
            p=subprocess.run([sys.executable,str(SCRIPT),'--output-dir',str(out),
                              '--credentials-dir',str(creds),'--include-gh-token',
                              '--recipient','age1FAKE'],env=env,capture_output=True,text=True,timeout=60)
            self.assertEqual(p.returncode,0,msg=p.stdout+'\n'+p.stderr)
            archive=list(out.glob('_metadata/*.tar.gz.age'))
            self.assertEqual(len(archive),1)
            repo_archives=list(out.glob('repositories/testuser/project/*.tar.gz.age'))
            self.assertEqual(len(repo_archives),1)
            self.assertEqual(len(list(out.rglob('*.sha256'))),2)
            self.assertIn('main-20261112T132000Z', repo_archives[0].name)
            self.assertIn('000000000000', repo_archives[0].name)
            self.assertFalse(list(out.glob('.github-backup-item-*')))
            self.assertFalse(list(out.glob('*.tar.gz')))
            with tarfile.open(archive[0],mode='r:gz') as tar:
                members=set(tar.getnames())
                required=[
                    'multi-backup/account/repositories.json',
                    'multi-backup/repositories/testuser/project/releases/1/assets/release.zip',
                    'multi-backup/repositories/testuser/project/ci/runs/123/logs.txt',
                    'multi-backup/repositories/testuser/project/ci/runs/123/artifacts/artifact.txt',
                    'multi-backup/repositories/testuser/project/settings/actions_secrets_METADATA_ONLY.json',
                    'multi-backup/credentials/github_cli_oauth_token.txt',
                    'multi-backup/credentials/supplied_by_user/original_secret.txt',
                    'multi-backup/backup_manifest.json',
                    'multi-backup/backup_errors.json',
                ]
                for path in required:self.assertIn(path,members)
                manifest=json.load(tar.extractfile('multi-backup/backup_manifest.json'))
                self.assertEqual(manifest['critical_failures'],0)
                self.assertEqual(manifest['counts']['ci_runs'],1)
                self.assertIn('testuser/project', manifest['repository_git_snapshots'])
                names=json.load(tar.extractfile('multi-backup/secret_inventory_VALUES_NOT_RECOVERABLE_BY_API.json'))
                self.assertIn('MY_SECRET',[x['name'] for x in names])
            with tarfile.open(repo_archives[0],mode='r:gz') as tar:
                gitmembers=set(tar.getnames())
                self.assertIn('multi-backup/repositories/testuser/project/git/project.git/HEAD', gitmembers)
                self.assertIn('multi-backup/repositories/testuser/project/git/project.wiki.git/HEAD', gitmembers)

    def test_python39_tar_stream_compatibility(self):
        """Reject the keyword unsupported by tarfile.open(w|gz) in Python 3.9."""
        ast.parse(SCRIPT.read_text(encoding='utf-8'), feature_version=(3, 9))
        streaming_class = runpy.run_path(str(SCRIPT), run_name='backup_test')['StreamingArchive']
        original_open = tarfile.open
        calls = []

        def python39_tarfile_open(*args, **kwargs):
            calls.append(kwargs.copy())
            if kwargs.get('mode') == 'w|gz' and 'compresslevel' in kwargs:
                raise TypeError("__init__() got an unexpected keyword argument 'compresslevel'")
            return original_open(*args, **kwargs)

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            tools = root / 'bin'
            tools.mkdir()
            fake_age = tools / 'age'
            fake_age.write_text(FAKE_AGE)
            fake_age.chmod(0o700)
            out = root / 'example.tar.gz.age'
            with mock.patch.dict(os.environ, {'PATH': str(tools) + os.pathsep + os.environ['PATH']}):
                with mock.patch.object(tarfile, 'open', side_effect=python39_tarfile_open):
                    with streaming_class(out, 'age1TEST') as encrypted:
                        encrypted.add_bytes('sample.txt', b'streamed gzip content')
            self.assertTrue(out.exists())
            self.assertTrue(any(c.get('mode') == 'w|gz' for c in calls))
            with original_open(out, mode='r:gz') as handle:
                self.assertEqual(handle.extractfile('sample.txt').read(), b'streamed gzip content')

    def test_repeated_identical_and_5_unique_limit(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); tools=root/'bin'; tools.mkdir()
            for executable,code in [('gh',FAKE_GH),('git',FAKE_GIT),('age',FAKE_AGE)]:
                f=tools/executable; f.write_text(code); f.chmod(0o700)
            out=root/'output'
            env=os.environ.copy(); env['PATH']=str(tools)+os.pathsep+env['PATH']
            cmd=[sys.executable,str(SCRIPT),'--output-dir',str(out),'--recipient','age1FAKE',
                 '--skip-ci-artifacts','--skip-ci-logs','--skip-release-assets','--skip-gists']
            def execute(other_ref):
                env['BACKUP_FAKE_OTHER_REF']=str(other_ref)
                p=subprocess.run(cmd,env=env,capture_output=True,text=True,timeout=60)
                self.assertEqual(p.returncode,0,msg=p.stdout+'\n'+p.stderr)
            execute(1)
            self.assertEqual(len(list(out.glob('repositories/testuser/project/*.tar.gz.age'))),1)
            execute(1)
            self.assertEqual(len(list(out.glob('repositories/testuser/project/*.tar.gz.age'))),1)
            for version in range(2,8):
                execute(version)
            files=list(out.glob('repositories/testuser/project/*.tar.gz.age'))
            self.assertEqual(len(files),5)
            self.assertEqual(len(list(out.glob('_metadata/*.tar.gz.age'))),5)
            infos=[json.loads(x.read_text()) for x in out.glob('repositories/testuser/project/*.age.json')]
            self.assertEqual(len({x['fingerprint'] for x in infos}),5)
            # CLI override: independent of the max-runs CI setting.
            cmd.extend(['--max-repo-copies', '3'])
            execute(8)
            self.assertEqual(len(list(out.glob('repositories/testuser/project/*.tar.gz.age'))),3)

if __name__ == '__main__':unittest.main()
