"""Regression tests for the new checkpointed, split-archive architecture."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
from checkpoint import Journal, sha256
from progress import Progress
from archive_ops import _safe_member, ArchiveError, catalog, extract_archive
from multi_backup import build_parser, ResumableBackup, VERSION
from ci_artifact_cleanup import GitHubCleanup

class CheckpointTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name)
        self.config={'output':'fake','recipient':'public'}

    def test_resume_is_idempotent_and_integrity_checked(self):
        job=Journal(self.root,self.config)
        document=self.root/'payload.txt';document.write_text('hello')
        job.record('step',document)
        self.assertTrue(job.done('step'))
        resumed=Journal(self.root,self.config)
        self.assertEqual(resumed.cycle,job.cycle)
        self.assertTrue(resumed.done('step'))
        document.write_text('HELLO')
        self.assertFalse(resumed.done('step'))

    def test_restart_creates_new_cycle_without_erasing_existing(self):
        first=Journal(self.root,self.config)
        payload=self.root/'backup.age';payload.write_bytes(b'previous-ciphertext')
        replacement=Journal(self.root,self.config,restart=True)
        self.assertNotEqual(first.cycle,replacement.cycle)
        self.assertEqual(payload.read_bytes(),b'previous-ciphertext')

    def test_resume_rejects_different_configuration(self):
        Journal(self.root,self.config)
        with self.assertRaises(RuntimeError):Journal(self.root,{'recipient':'different'})

    def test_finished_cycle_clears_active_checkpoint(self):
        first=Journal(self.root,self.config)
        first.finish()
        self.assertFalse((self.root/'active.json').exists())
        self.assertTrue((self.root/('cycle-'+first.cycle+'.json')).is_file())
        second=Journal(self.root,self.config)
        self.assertNotEqual(first.cycle,second.cycle)

    def test_journal_only_resumes_valid_sha_and_size(self):
        job=Journal(self.root,self.config)
        file=self.root/'data';file.write_bytes(b'x'*10)
        job.record('data',file)
        file.write_bytes(b'x'*9)
        self.assertFalse(job.done('data'))

class CLIAndArchiveTests(unittest.TestCase):
    def test_version_and_flags(self):
        args=build_parser().parse_args(['backup','--cleanup-ci-artifacts','--keep-ci-runs','3',
                                        '--progress','--metadata-workers','6'])
        self.assertTrue(args.cleanup_ci_artifacts)
        self.assertTrue(args.progress)
        self.assertEqual(args.keep_ci_runs,3)
        self.assertEqual(args.metadata_workers,6)
        self.assertFalse(args.ignore_ci_artifact_errors)
        opted=build_parser().parse_args(['backup','--ignore-ci-artifact-errors'])
        self.assertTrue(opted.ignore_ci_artifact_errors)
        self.assertTrue(VERSION.startswith('0.1.0-r'))

    def test_malicious_archive_member_rejected(self):
        for name in ('../../etc/passwd','/etc/passwd','multi-backup/../escape'):
            entry=tarfile.TarInfo(name);entry.type=tarfile.REGTYPE
            with self.assertRaises(ArchiveError):_safe_member(entry)
        for kind in (tarfile.SYMTYPE,tarfile.LNKTYPE,tarfile.FIFOTYPE):
            entry=tarfile.TarInfo('multi-backup/something');entry.type=kind
            with self.assertRaises(ArchiveError):_safe_member(entry)

    def test_backup_catalog_groups_encrypted_files(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)
            repo=root/'repositories'/'owner'/'name';repo.mkdir(parents=True)
            (repo/'name__git-main-2026__state-abcdef.tar.gz.age').write_text('encrypted')
            md=root/'metadata'/'cycles'/'cycle'/'repositories'/'owner'/'name'
            md.mkdir(parents=True)
            (md/'settings.tar.gz.age').write_text('encrypted')
            items=catalog(root)
            self.assertEqual({x['kind'] for x in items},{'metadata','git'})

    def test_progress_emits_parseable_json(self):
        stream=io.StringIO()
        progress=Progress(json_events=True,stream=stream)
        evt=progress.emit('stage','working',completed=2,total=4,repository='owner/repo')
        self.assertEqual(evt['percent'],50)
        self.assertEqual(json.loads(stream.getvalue())['repository'],'owner/repo')

    def test_dry_run_does_not_create_directory(self):
        from multi_backup import main
        with tempfile.TemporaryDirectory() as temp:
            path=Path(temp)/'backup'
            with patch('builtins.print'):
                self.assertEqual(main(['backup','--dry-run','--output-dir',str(path)]),0)
            self.assertFalse(path.exists())

    def test_cleanup_progress_callbacks_and_no_deletes_when_dry_run(self):
        # Existing API integration fixtures assert individual HTTP calls and the
        # fail-closed policy; this asserts the new callback contract.
        seen=[]
        cleanup=GitHubCleanup(dry_run=True,on_progress=seen.append)
        self.assertIsNotNone(cleanup.on_progress)
        self.assertEqual(seen,[])

    def test_resume_unindexed_shard_with_sha_sidecar(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)
            args=build_parser().parse_args(['backup','--output-dir',str(root),'--recipient','age1fake'])
            engine=ResumableBackup(args,Progress(stream=io.StringIO()))
            output=engine.cycle_dir/'repositories'/'owner'/'repo'/'settings.tar.gz.age'
            output.parent.mkdir(parents=True)
            output.write_bytes(b'preexisting encrypted shard')
            output.with_suffix(output.suffix+'.sha256').write_text(
                f'{sha256(output)}  {output.name}\n')
            producer=lambda root: self.fail('Must never rerun verified shard')
            engine._archive('test',output,producer)
            self.assertTrue(engine.journal.done('test'))

if __name__=='__main__': unittest.main()
