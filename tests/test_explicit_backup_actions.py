"""Explicit Start/Resume, immutable session names and guarded local archive delete."""
from __future__ import annotations

import io
import json
import tempfile
import unittest
from pathlib import Path

from backup_sessions import start_dropbox_session, dropbox_active_manifest, finalize_dropbox_session
from archive_management import ArchiveDeletionError, deletion_plan, delete_local_archive
from checkpoint import Journal, sha256


class StartResumeTests(unittest.TestCase):
    def test_explicit_resume_preserves_id_and_name(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root)
            first = Journal(path / '.state', {'test': True}, restart=True)
            same = Journal(path / '.state', {'test': True}, require_active=True)
            self.assertEqual(first.cycle, same.cycle)
            self.assertTrue(same.resumed)
            self.assertEqual(first.path, same.path)

    def test_resume_without_checkpoint_never_starts_new_backup(self):
        with tempfile.TemporaryDirectory() as root:
            directory=Path(root) / '.state'
            with self.assertRaisesRegex(RuntimeError,'No incomplete backup'):
                Journal(directory, {}, require_active=True)
            self.assertFalse((directory / 'active.json').exists())

    def test_explicit_start_rotates_checkpoint_instead_of_reusing_or_deleting(self):
        with tempfile.TemporaryDirectory() as root:
            directory=Path(root) / '.state'
            old=Journal(directory, {'test': True},restart=True)
            new=Journal(directory, {'test': True},restart=True)
            self.assertNotEqual(old.cycle,new.cycle)
            parked=directory / ('abandoned-'+old.cycle+'.json')
            self.assertTrue(parked.is_file())
            self.assertEqual(json.loads(parked.read_text('utf-8'))['cycle'],old.cycle)
            self.assertEqual(json.loads((directory/'active.json').read_text('utf-8'))['cycle'],
                             new.cycle)

    def test_dropbox_resume_keeps_exact_manifest_filename(self):
        with tempfile.TemporaryDirectory() as root:
            path=Path(root)
            original,resumed=start_dropbox_session(path,'age1test','/photos',restart=True)
            self.assertFalse(resumed)
            current,resumed=start_dropbox_session(path,'age1test','/photos',resume=True)
            self.assertTrue(resumed)
            self.assertEqual(original,current)
            self.assertEqual(dropbox_active_manifest(path),original)
            with self.assertRaisesRegex(RuntimeError,'does not match'):
                start_dropbox_session(path,'age1different','/photos',resume=True)
            newer,_=start_dropbox_session(path,'age1test','/photos',restart=True)
            self.assertNotEqual(original,newer)
            self.assertTrue(list((path/'.state').glob('dropbox-abandoned-*.json')))

    def test_dropbox_finalization_requires_verified_manifest(self):
        with tempfile.TemporaryDirectory() as root:
            path=Path(root)
            name,_=start_dropbox_session(path,'age1test','',restart=True)
            dest=path/'dropbox'/'manifests'/name
            dest.parent.mkdir(parents=True)
            dest.write_bytes(b'encrypted')
            with self.assertRaisesRegex(RuntimeError,'incomplete'):
                finalize_dropbox_session(path,dest)
            self.assertEqual(dropbox_active_manifest(path),name)
            dest.with_suffix(dest.suffix+'.sha256').write_text(
                sha256(dest)+'  '+dest.name+'\n')
            finalize_dropbox_session(path,dest)
            self.assertIsNone(dropbox_active_manifest(path))


class LocalDeletionTests(unittest.TestCase):
    def _make_archive(self,root):
        file=Path(root)/'repositories'/'owner'/'repo'/'state__git-main-20261114__state-000.tar.gz.age'
        file.parent.mkdir(parents=True)
        file.write_bytes(b'ciphertext, do not inspect')
        return file

    def test_deletes_only_registered_archive_and_paired_sidecars(self):
        with tempfile.TemporaryDirectory() as root:
            path=Path(root); archived=self._make_archive(path)
            sha=archived.with_name(archived.name+'.sha256')
            sha.write_text(sha256(archived))
            other=archived.with_name('other.txt')
            other.write_bytes(b'not part of archive')
            self.assertEqual(len(deletion_plan(path,archived)),2)
            removed=delete_local_archive(path,archived,confirmation=archived.name)
            self.assertEqual(len(removed),2)
            self.assertFalse(archived.exists())
            self.assertFalse(sha.exists())
            self.assertTrue(other.exists())
            log=path/'.state'/'archive-deletions.jsonl'
            entry=json.loads(log.read_text().splitlines()[-1])
            self.assertTrue(entry['complete'])
            self.assertEqual(entry['operation'],'local-delete-only')

    def test_wrong_confirmation_and_outside_root_are_blocked(self):
        with tempfile.TemporaryDirectory() as root, tempfile.TemporaryDirectory() as outside:
            archived=self._make_archive(root)
            with self.assertRaisesRegex(ArchiveDeletionError,'Confirmation'):
                delete_local_archive(root,archived,confirmation='wrong')
            external=self._make_archive(outside)
            with self.assertRaisesRegex(ArchiveDeletionError,'outside'):
                deletion_plan(root,external)
            self.assertTrue(archived.is_file())

    def test_symlink_and_active_resume_checkpoint_are_protected(self):
        with tempfile.TemporaryDirectory() as root:
            path=Path(root); archived=self._make_archive(path)
            state=path/'.state'
            state.mkdir()
            (state/'active.json').write_text(json.dumps({
                'steps':{'archive':{'path':str(archived)}}}))
            with self.assertRaisesRegex(ArchiveDeletionError,'unfinished'):
                deletion_plan(path,archived)
            (state/'active.json').unlink()
            symlink=archived.with_name('link__git-main-20261114__state-000.tar.gz.age')
            symlink.symlink_to(archived)
            with self.assertRaises(ArchiveDeletionError):
                deletion_plan(path,symlink)
            self.assertTrue(archived.is_file())

    def test_active_job_prevents_delete(self):
        with tempfile.TemporaryDirectory() as root:
            archived=self._make_archive(root)
            with self.assertRaisesRegex(ArchiveDeletionError,'running'):
                delete_local_archive(root,archived,confirmation=archived.name,in_use=True)
            self.assertTrue(archived.is_file())


if __name__=='__main__':
    unittest.main()
