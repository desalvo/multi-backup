"""Offline tests; never call the real GitHub API or delete real artifacts."""
import base64
import json
import subprocess
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from ci_artifact_cleanup import GitHubCleanup, select_artifacts


def sample_run(i, branch='main', event='push', status='completed', workflow_id=8):
    return dict(id=i, head_branch=branch, event=event, status=status,
                workflow_id=workflow_id, created_at='2026-11-%02dT00:00:00Z' % min(i, 28))


def sample_artifact(i, rid, name='test-results', expired=False):
    return dict(id=i, name=name, expired=expired, workflow_run={'id': rid})


class SelectionTests(unittest.TestCase):
    def setUp(self):
        self.repo = {'full_name': 'example/whatever', 'default_branch': 'main'}
        self.workflows = {8: {'available': True, 'path': '.github/workflows/ci.yml', 'source': 'on: push\n'}}

    def plan(self, runs, artifacts, *, workflows=None, releases=None, tags=None, keep=5):
        return select_artifacts(self.repo, runs, artifacts, set(tags or ()), releases or [],
                                self.workflows if workflows is None else workflows, keep)

    def test_default_branch_last_five_and_last_one_per_tag(self):
        runs = [sample_run(i) for i in range(1, 8)]
        runs += [sample_run(8, 'v1.0.0'), sample_run(9, 'v1.0.0'),
                 sample_run(10, 'pr-42', event='pull_request'),
                 sample_run(11, 'feature-x', status='in_progress')]
        artifacts = [sample_artifact(100 + r['id'], r['id']) for r in runs]
        p = self.plan(runs, artifacts, tags={'v1.0.0'})
        kept = {a['run_id'] for a in p['artifacts'] if a['action'] == 'keep'}
        self.assertEqual(kept, {3, 4, 5, 6, 7, 9, 11})
        self.assertEqual(p['counts']['eligible_artifacts'], 4)
        self.assertEqual(p['counts']['protected_artifacts'], 7)

    def test_protects_historical_package_even_on_nondefault_branch(self):
        p = self.plan([sample_run(1, 'release-old'), sample_run(2, 'pr-42', event='pull_request')],
                      [sample_artifact(1, 1, 'archive-rpm-repository'),
                       sample_artifact(2, 2, 'coverage-report')])
        self.assertEqual(p['artifacts'][0]['action'], 'keep')
        self.assertEqual(p['artifacts'][1]['action'], 'delete')
        self.assertIn('potential-distribution-artifact-not-verifiably-published', p['artifacts'][0]['reasons'])

    def test_exact_published_asset_allows_old_artifact_cleanup(self):
        arts = [sample_artifact(1, 1, 'my-package.zip'), sample_artifact(2, 2, 'my-package.zip')]
        arts[0]['digest'] = 'sha256:' + 'a' * 64
        arts[1]['digest'] = 'sha256:' + 'a' * 64
        p = self.plan([sample_run(1, 'v1'), sample_run(2, 'v1')], arts,
                      tags={'v1'}, releases=[{'tag': 'v1', 'assets': {'my-package.zip': 'sha256:' + 'a' * 64}}])
        self.assertEqual([x['action'] for x in p['artifacts']], ['delete', 'keep'])
        # The same filename without verified matching digests is NOT sufficient.
        arts[0].pop('digest')
        p2 = self.plan([sample_run(1, 'v1'), sample_run(2, 'v1')], arts,
                      tags={'v1'}, releases=[{'tag': 'v1', 'assets': {'my-package.zip': 'sha256:' + 'a' * 64}}])
        self.assertEqual([x['action'] for x in p2['artifacts']], ['keep', 'keep'])

    def test_unknown_workflow_keeps_artifact(self):
        p = self.plan([sample_run(1, workflow_id=10)], [sample_artifact(1, 1)], workflows={})
        self.assertEqual(p['artifacts'][0]['action'], 'keep')
        self.assertIn('workflow-source-unavailable', p['artifacts'][0]['reasons'])

    def test_cross_run_consumer_keeps_old_artifacts(self):
        wf = {8: {'available': True, 'source': 'on:\n  workflow_run:\n    workflows: [Build]\nsteps:\n  - uses: actions/download-artifact@v4\n    with:\n      run-id: ${{ github.event.workflow_run.id }}'}}
        p = self.plan([sample_run(1, 'other')], [sample_artifact(1, 1)], workflows=wf)
        self.assertEqual(p['artifacts'][0]['action'], 'keep')
        self.assertTrue(p['workflow_cross_run_consumers_detected'])

    def test_unknown_active_and_expired_protected(self):
        p = self.plan([sample_run(1, 'other', status='queued')],
                      [sample_artifact(1, 1), sample_artifact(2, 123), sample_artifact(3, 1, expired=True)])
        self.assertTrue(all(x['action'] == 'keep' for x in p['artifacts']))

    def test_duplicate_ids_fail_closed(self):
        with self.assertRaises(Exception):
            self.plan([sample_run(1)], [sample_artifact(99, 1), sample_artifact(99, 1)])


class GitHubMockAPI:
    """Simulate gh api; supports inventory, revalidation and mutation tracking."""
    def __init__(self, fail_at=None, fail_delete=False):
        self.deletes = []
        self.fail_at = fail_at
        self.fail_delete = fail_delete
        self.runs = [sample_run(1, 'main'), sample_run(2, 'main'), sample_run(3, 'main')]
        self.artifacts = [sample_artifact(101, 1), sample_artifact(102, 2), sample_artifact(103, 3)]
        self.source = 'name: CI\non: push\njobs: {}\n'

    def __call__(self, cmd, **kwargs):
        assert cmd[:2] == ['gh', 'api'], cmd
        endpoint = cmd[-1]
        if self.fail_at and self.fail_at in endpoint:
            return subprocess.CompletedProcess(cmd, 1, stdout=b'', stderr=b'access denied')
        if '--method' in cmd:
            if self.fail_delete:
                return subprocess.CompletedProcess(cmd, 1, stdout=b'', stderr=b'403')
            aid = int(endpoint.rsplit('/', 1)[1]); self.deletes.append(aid)
            return subprocess.CompletedProcess(cmd, 0, stdout=b'', stderr=b'')
        data = None
        if endpoint.endswith('/actions/runs?per_page=100'):
            data = {'workflow_runs': self.runs}
        elif endpoint.endswith('/actions/artifacts?per_page=100'):
            data = {'artifacts': self.artifacts}
        elif endpoint.endswith('/tags?per_page=100') or endpoint.endswith('/releases?per_page=100'):
            data = []
        elif endpoint.endswith('/actions/workflows?per_page=100'):
            data = {'workflows': [{'id': 8, 'path': '.github/workflows/ci.yml'}]}
        elif '/contents/.github/workflows/ci.yml?' in endpoint:
            data = {'content': base64.b64encode(self.source.encode()).decode(), 'encoding': 'base64'}
        elif '/actions/runs/' in endpoint:
            rid = int(endpoint.rsplit('/', 1)[1]); data = next(r for r in self.runs if r['id'] == rid)
        elif '/actions/artifacts/' in endpoint:
            aid = int(endpoint.rsplit('/', 1)[1]); data = next(a for a in self.artifacts if a['id'] == aid)
        if data is None:
            return subprocess.CompletedProcess(cmd, 1, stdout=b'', stderr=b'not found')
        if '--slurp' in cmd:
            data = [data]
        return subprocess.CompletedProcess(cmd, 0, stdout=json.dumps(data).encode(), stderr=b'')


class APIWorkflowTests(unittest.TestCase):
    def setUp(self):
        self.repo = {'full_name': 'example/whatever', 'default_branch': 'main'}

    def test_dry_run_never_deletes(self):
        gh = GitHubMockAPI()
        report = GitHubCleanup(keep_ci_runs=1, dry_run=True, runner=gh).run(self.repo)
        self.assertEqual(report['counts']['eligible_artifacts'], 2)
        self.assertEqual(gh.deletes, [])

    def test_apply_only_old_artifacts(self):
        gh = GitHubMockAPI()
        report = GitHubCleanup(keep_ci_runs=1, dry_run=False, runner=gh).run(self.repo)
        self.assertEqual(gh.deletes, [101, 102])
        self.assertEqual(report['keep_run_ids'], [3])

    def test_api_failure_skips_repo_without_any_deletes(self):
        gh = GitHubMockAPI(fail_at='/releases?')
        report = GitHubCleanup(keep_ci_runs=1, runner=gh).run(self.repo)
        self.assertTrue(report['skipped_due_to_error'])
        self.assertIsNone(report['keep_run_ids'])
        self.assertEqual(gh.deletes, [])

    def test_delete_failure_stops_and_disables_ci_backup_filter(self):
        gh = GitHubMockAPI(fail_delete=True)
        report = GitHubCleanup(keep_ci_runs=1, runner=gh).run(self.repo)
        self.assertEqual(gh.deletes, [])
        self.assertEqual(report['failed_artifact_ids'], [101])
        self.assertIsNone(report['keep_run_ids'])


if __name__ == '__main__':
    unittest.main()
