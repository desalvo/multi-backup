"""Regression suite for recoverable Actions artifact errors and accurate resume."""
from __future__ import annotations

import io
import json
import tempfile
import unittest
import zipfile
from pathlib import Path

from checkpoint import Journal
from ci_download import ArtifactDownloadError, retrieve_artifact


class ArtifactDownloadTests(unittest.TestCase):
    ARTIFACT = {'id': 13676752335, 'name': 'ci-results', 'expired': False}

    def test_valid_binary_zip_is_preserved_without_fallback(self):
        buf = io.BytesIO()
        with zipfile.ZipFile(buf, 'w') as archive:
            archive.writestr('results/pass.txt', 'ok')
        calls = []
        def gh(args, label, **kwargs):
            calls.append(args)
            if args[0] == 'api':
                kwargs['file'].write(buf.getvalue())
                return b''
            self.fail('Valid binary artifact should not use fallback')
        with tempfile.TemporaryDirectory() as tmp:
            result = retrieve_artifact('example/project', 123, self.ARTIFACT,
                                       [self.ARTIFACT], tmp, gh)
            self.assertEqual(result.name, '13676752335.zip')
            self.assertTrue(zipfile.is_zipfile(result))
        self.assertEqual(len(calls), 1)

    def test_gh_download_retries_and_continues_after_transient_failure(self):
        calls = []
        delays = []
        def gh(args, label, **kwargs):
            calls.append(args[0:2])
            if args[0] == 'api':
                kwargs['file'].write(b'HTTP 302 but no ZIP')
                return b''
            if len(calls) < 4:
                return None
            Path(args[-1], 'file.txt').write_text('recovered artifact', encoding='utf-8')
            return b'ok'
        with tempfile.TemporaryDirectory() as tmp:
            result = retrieve_artifact(
                'example/project', 123, self.ARTIFACT, [self.ARTIFACT], tmp,
                gh, sleeper=delays.append, delays=(1, 3, 7))
            self.assertTrue((result / 'file.txt').is_file())
            self.assertEqual(delays, [1, 3])
            self.assertFalse((Path(tmp) / '13676752335.zip').exists())

    def test_all_retries_fail_but_no_corrupt_archive_is_left(self):
        def gh(args, label, **kwargs):
            if args[0] == 'api':
                kwargs['file'].write(b'not a zip')
            return None
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaisesRegex(ArtifactDownloadError, 'remains retryable'):
                retrieve_artifact('example/project', 123, self.ARTIFACT,
                                  [self.ARTIFACT], tmp, gh,
                                  sleeper=lambda n: None, delays=(0, 0))
            self.assertEqual(list(Path(tmp).iterdir()), [])

    def test_ambiguous_fallback_name_is_rejected(self):
        called = []
        def gh(args, label, **kwargs):
            called.append(args[0])
            return None
        dup = dict(self.ARTIFACT, id=998)
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaisesRegex(ArtifactDownloadError, 'ambiguous'):
                retrieve_artifact('example/project', 123, self.ARTIFACT,
                                  [self.ARTIFACT, dup], tmp, gh,
                                  sleeper=lambda n: None)
            self.assertEqual(called, ['api'])

    def test_isolated_ci_run_failure_and_partial_checkpoint_contract(self):
        src = (Path(__file__).resolve().parents[1] / 'multi_backup.py').read_text('utf-8')
        self.assertIn("self.counts['ci_candidates_skipped']", src)
        self.assertIn("if status=='partial':", src)
        self.assertIn("last-incomplete.json", src)
        self.assertIn("retrieve_artifact(", src)

    def test_missing_ci_run_is_replaced_by_older_successful_run(self):
        from types import SimpleNamespace
        from multi_backup import ResumableBackup
        from progress import Progress
        job=object.__new__(ResumableBackup)
        job.args=SimpleNamespace(max_runs_per_repo=3,metadata_workers=2,
                                 recipient='age1example',ignore_ci_artifact_errors=False)
        job.progress=Progress(stream=io.StringIO())
        job.counts={'errors':0}
        job.ci_issues=[]
        job.ci_shortfalls={}
        job._ci_index=lambda slug: [{'id':i} for i in (9,8,7,6,5)]
        attempted=[]
        def archive_one(slug, run):
            attempted.append(run['id'])
            if run['id']==8:
                raise ArtifactDownloadError('CI artifact temporarily unavailable')
            return 'new'
        job._ci_run=archive_one
        job._ci('owner/repo')
        self.assertEqual(job.counts['errors'],0)
        self.assertEqual(job.counts['ci_runs_fully_preserved'],3)
        self.assertEqual(len(job.ci_issues),1)
        self.assertIn(8,attempted)
        self.assertEqual(len(attempted),4)
        self.assertEqual(job.ci_shortfalls,{})

    def test_ignore_errors_allows_shortfall_but_never_calls_it_complete(self):
        from types import SimpleNamespace
        from multi_backup import ResumableBackup
        from progress import Progress
        job=object.__new__(ResumableBackup)
        job.args=SimpleNamespace(max_runs_per_repo=3,metadata_workers=1,
                                 recipient='age1example',ignore_ci_artifact_errors=True)
        job.progress=Progress(stream=io.StringIO())
        job.counts={'errors':0}
        job.ci_issues=[]
        job.ci_shortfalls={}
        job._ci_index=lambda slug: [{'id':i} for i in (9,8,7)]
        job._ci_run=lambda slug, run: 'partial'
        job._ci('owner/repo')
        self.assertEqual(job.counts['errors'],0)
        self.assertEqual(job.ci_shortfalls,{'owner/repo':3})
        self.assertEqual(job.counts['ci_runs_fully_preserved'],0)
        self.assertEqual(len(job.ci_issues),3)

    def test_strict_mode_records_shortfall_for_resume(self):
        from types import SimpleNamespace
        from multi_backup import ResumableBackup
        from progress import Progress
        job=object.__new__(ResumableBackup)
        job.args=SimpleNamespace(max_runs_per_repo=2,metadata_workers=1,
                                 recipient='age1example',ignore_ci_artifact_errors=False)
        job.progress=Progress(stream=io.StringIO())
        job.counts={'errors':0}
        job.ci_issues=[]
        job.ci_shortfalls={}
        job._ci_index=lambda slug: [{'id':7}, {'id':6}]
        def fail(slug, run):
            raise ArtifactDownloadError('transient failure')
        job._ci_run=fail
        job._ci('owner/repo')
        self.assertEqual(job.ci_shortfalls,{'owner/repo':2})
        self.assertEqual(job.counts['errors'],2)

    def test_journal_reports_existing_resume_without_rotating_cycle(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp)
            first = Journal(folder, {'recipient': 'age1test'})
            self.assertFalse(first.resumed)
            payload = folder / 'finished-unit.bin'
            payload.write_bytes(b'checked')
            first.record('one', payload)
            second = Journal(folder, {'recipient': 'age1test'})
            self.assertTrue(second.resumed)
            self.assertEqual(first.cycle, second.cycle)
            self.assertTrue(second.done('one'))


if __name__ == '__main__':
    unittest.main()
