#!/usr/bin/env python3
"""Multi Backup 0.1.0-r001: resumable, encrypted, per-repository snapshots.

This is a disaster-recovery exporter, not a complete automated account restore.
Only --cleanup-ci-artifacts with no --dry-run writes to GitHub (DELETE artifacts).
"""
from __future__ import annotations
import argparse
import concurrent.futures
import datetime as dt
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace

from checkpoint import Journal, atomic_json, sha256
from ci_download import retrieve_artifact, ArtifactDownloadError
from progress import Progress
from archive_ops import (ArchiveError, catalog, checkout_git, extract_archive, list_members,
                         mount_archive, read_member, unmount_archive)
from migration import migrate_legacy, legacy_archives
from ci_artifact_cleanup import GitHubCleanup
from github_engine import Backup, StreamingArchive, check_prerequisites

from version import VERSION


def checksum_sidecar(path: Path):
    path.with_suffix(path.suffix+'.sha256').write_text(
        f'{sha256(path)}  {path.name}\n', encoding='utf-8')


class ResumableBackup:
    def __init__(self, args, progress=None):
        self.args = args
        self.output = args.output_dir.resolve()
        self.staging = (args.staging_dir or args.output_dir).resolve()
        for target in (self.output, self.staging):
            if target.is_symlink():
                raise RuntimeError('Output and staging directories must not be symlinks')
            target.mkdir(parents=True,exist_ok=True, mode=0o700)
            if target.stat().st_mode & 0o077:
                raise RuntimeError('Private output/staging directories must have mode 0700')
        self.progress = progress or Progress(args.progress, args.json_progress)
        cfg = {k: getattr(args, k) for k in (
            'recipient', 'max_copies_per_repo', 'max_runs_per_repo',
            'cleanup_ci_artifacts', 'cleanup_ci_dry_run', 'keep_ci_runs',
            'skip_gists', 'skip_ci_logs', 'skip_ci_artifacts', 'skip_release_assets',
            'skip_issue_details', 'skip_issue_events', 'skip_org_settings',
            'include_gh_token', 'metadata_workers', 'purge_obsolete_metadata')}
        cfg['github_user'] = getattr(args,'github_user',None)
        cfg['credentials_dir'] = str(args.credentials_dir or '')
        self.journal = Journal(self.output / '.state',cfg,restart=args.restart,
                               require_active=args.resume)
        self.cycle_dir = self.output / 'metadata' / 'cycles' / self.journal.cycle
        self.cycle_dir.mkdir(parents=True,exist_ok=True,mode=0o700)
        self.backup = Backup(args, self.staging / '.multi-backup-staging-placeholder')
        self.counts = {'repositories':0,'metadata_archives':0,'ci_runs_archived':0,
                       'ci_cache_reused':0,'ci_deleted':0,'ci_protected':0,
                       'ci_candidates':0,'errors':0}
        self.cleanup_reports = {}
        self.repository_snapshots = {}
        self.ci_issues = []  # failed candidate IDs; preserved in the encrypted manifest
        self.ci_shortfalls = {}  # repository: missing fully archived CI runs
        if self.journal.resumed:
            previous=len(self.journal.data.get('steps', {}))
            self.progress.emit(
                'resume-start',
                f'RIPRESA backup esistente: ciclo {self.journal.cycle}; '
                f'{previous} checkpoint registrati, verifica SHA-256 '
                'dei segmenti salvati durante la ripresa. '
                'Nessun archivio verificato verrà sovrascritto.')
            # A manifest from an older incomplete cycle must be regenerated
            # after missing CI artifacts have been recovered.
            if self.journal.data['steps'].pop('manifest', None):
                atomic_json(self.journal.path, self.journal.data)
        else:
            self.progress.emit('cycle-start',
                               f'Nuovo ciclo di backup {self.journal.cycle}')

    def _archive(self, name: str, destination: Path, producer, *, archive_prefix='multi-backup',
                 remove=None):
        """Producer fills a private temp dir. Encrypt to .partial then publish atomically."""
        if self.journal.done(name):
            self.progress.emit('resume','Verified, skipped '+name)
            return destination
        if destination.exists():
            # Power loss may happen between publishing the shard and recording
            # its checkpoint: recover by checking the atomic SHA sidecar.
            sidecar = destination.with_suffix(destination.suffix+'.sha256')
            if sidecar.is_file() and sidecar.read_text('utf-8').split()[0] == sha256(destination):
                self.journal.record(name, destination)
                self.progress.emit('resume', 'Recovered completed shard '+name)
                return destination
            # Never overwrite or silently remove a possibly important archive.
            orphaned = self.output / '.state' / 'orphaned'
            orphaned.mkdir(parents=True, exist_ok=True, mode=0o700)
            destination.rename(orphaned / (self.journal.cycle + '-' + sha256(destination)[:16] + '.age'))
            sidecar.unlink(missing_ok=True)
        destination.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
        with tempfile.TemporaryDirectory(dir=self.staging,prefix='.multi-backup-step-') as tmp:
            temp = Path(tmp)
            root = temp / 'multi-backup'
            root.mkdir(mode=0o700)
            self.backup.root = root
            producer(root)
            partial = destination.with_name(destination.name+'.partial')
            partial.unlink(missing_ok=True)  # Previous interrupted, incomplete write.
            try:
                with StreamingArchive(partial,self.args.recipient) as archive:
                    for entry in sorted(root.iterdir()):
                        archive.add_tree(entry,'multi-backup/'+entry.name)
                os.replace(partial,destination)
                checksum_sidecar(destination)
                self.journal.record(name,destination)
                self.counts['metadata_archives'] += 1
            finally:
                partial.unlink(missing_ok=True)
        return destination

    def _account(self):
        account = self.cycle_dir / 'common' / 'account.tar.gz.age'
        def producer(root):
            self.backup.backup_account()
            repos = self.backup.get_repos()
            if not repos: raise RuntimeError('GitHub returned no accessible repositories; refusing an empty cycle')
            # Only persist the discovery record upon success.
            self.journal.data['repositories'] = repos
            atomic_json(self.journal.path,self.journal.data)
        if not self.journal.done('account'):
            self._archive('account',account,producer)
        else:
            self.progress.emit('resume','Account snapshot already complete')
        repos = self.journal.data.get('repositories')
        if not isinstance(repos,dict): raise RuntimeError('Missing repository discovery checkpoint')
        return repos

    def _cleanup(self, repositories):
        if not self.args.cleanup_ci_artifacts:
            return
        totals = {'deleted':0,'eligible':0,'protected':0,'errors':0,'repos':0}
        for index,(slug,repo) in enumerate(repositories.items(),1):
            name='cleanup:'+slug
            location=self.cycle_dir / 'cleanup' / (slug.replace('/','__')+'.json')
            if self.journal.done(name):
                report=json.loads(location.read_text('utf-8'))
            else:
                report = GitHubCleanup(
                    self.args.keep_ci_runs,dry_run=self.args.cleanup_ci_dry_run,
                    on_progress=lambda event, repo=slug: self._cleanup_event(repo,event)
                ).run(repo)
                atomic_json(location,report)
                self.journal.record(name,location)
            self.cleanup_reports[slug]=report
            numbers=report.get('counts',{})
            totals['repos']+=1
            totals['eligible']+=numbers.get('eligible_artifacts',0)
            totals['protected']+=numbers.get('protected_artifacts',0)
            totals['deleted']+=len(report.get('deleted_artifact_ids',[]))
            totals['errors']+=len(report.get('failed_artifact_ids',[]))+bool(report.get('skipped_due_to_error'))
            self.progress.emit('cleanup',
                f'{slug}: eligible={numbers.get("eligible_artifacts",0)} '
                f'protected={numbers.get("protected_artifacts",0)} '
                f'deleted={len(report.get("deleted_artifact_ids",[]))} '
                f'errors={len(report.get("failed_artifact_ids",[]))}',
                repository=slug,sub_progress={'completed':index,'total':len(repositories)},
                summary={'per_repository':True, 'deleted':len(report.get('deleted_artifact_ids',[])),
                         'protected':numbers.get('protected_artifacts',0)})
        self.counts.update({'ci_deleted':totals['deleted'],'ci_protected':totals['protected'],
                            'ci_candidates':totals['eligible']})
        self.counts['errors']+=totals['errors']
        self.progress.emit('cleanup-total',
            f"Repositories={totals['repos']}; eligible={totals['eligible']}; "
            f"protected={totals['protected']}; deleted={totals['deleted']}; errors={totals['errors']}",
            summary=totals)

    def _cleanup_event(self, repository, event):
        self.progress.emit('cleanup-item',
            f"{repository} {event.get('status')}: {event.get('name')} "
            f"({event.get('done')}/{event.get('total')})", repository=repository,
            artifact_id=event.get('id'), artifact_status=event.get('status'),
            artifact_progress={'done':event.get('done'), 'total':event.get('total')})

    def _stage(self, slug, repo, task, fn):
        dest=self.cycle_dir / 'repositories' / slug / (task+'.tar.gz.age')
        name=f'repo:{slug}:{task}'
        def producer(root):
            folder=root / 'repositories' / slug
            folder.mkdir(parents=True,exist_ok=True,mode=0o700)
            fn(folder)
        self._archive(name,dest,producer)

    def _git(self,slug,repo):
        label='git:'+slug
        marker=self.cycle_dir / 'git-indices' / slug / 'index.json'
        if self.journal.done(label):
            record=json.loads(marker.read_text('utf-8'))
            archive=self.output / record['relative_archive']
            if archive.is_file() and sha256(archive)==record['archive_sha256']:
                self.repository_snapshots[slug]=record
                self.progress.emit('resume','Verified Git snapshot '+slug,repository=slug)
                return
        with tempfile.TemporaryDirectory(dir=self.staging,prefix='.multi-backup-git-') as tmp:
            folder=Path(tmp) / 'multi-backup' / 'repositories' / slug
            folder.mkdir(parents=True,exist_ok=True)
            self.backup.root=Path(tmp)/'multi-backup'
            record=self.backup.backup_git_snapshot(slug,folder,repo,self.output/'repositories'/slug)
            if record is None:
                self.counts['errors']+=1
                raise RuntimeError('Git mirror failed: '+slug)
            archive=self.output / record['relative_archive']
            if sha256(archive)!=record['archive_sha256']:
                raise RuntimeError('Git archive integrity check failed')
            atomic_json(marker,record)
            self.journal.record(label,marker)
            self.repository_snapshots[slug]=record

    def _ci_index(self,slug):
        output=self.cycle_dir/'repositories'/slug/'ci'/'runs-index.json'
        key=f'repo:{slug}:ci-index'
        folder=output.parent
        folder.mkdir(parents=True,exist_ok=True,mode=0o700)
        previous=None
        if self.journal.done(key):
            previous=json.loads(output.read_text('utf-8'))
        # Do not truncate the candidate list to N *before* downloading: a
        # missing Actions artifact should be replaced with an older run.
        candidate_file=folder/'runs-candidates.json'
        runs=self.backup.api(
            f'/repos/{slug}/actions/runs?per_page=100',
            candidate_file,collection='workflow_runs',optional=previous is not None)
        if runs is None:
            if previous is None:
                raise RuntimeError('Cannot list workflow runs for '+slug)
            self.progress.emit(
                'ci-warning',f'{slug}: elenco GitHub non aggiornabile; '
                'uso la lista CI verificata del checkpoint (rimpiazzi limitati)',
                repository=slug)
            return previous
        report=self.cleanup_reports.get(slug)
        if report and not report.get('dry_run') and report.get('keep_run_ids') is not None:
            allowed=set(report['keep_run_ids'])
            runs=[r for r in runs if r.get('id') in allowed]
        runs.sort(key=lambda r:(r.get('created_at') or '',r.get('id') or 0),
                  reverse=True)
        if previous is not None:
            # Never replace an originally selected run with a newer one simply
            # because new GitHub CI executions appeared after interruption.
            original_ids={r.get('id') for r in previous}
            oldest=min((r.get('created_at') or '' for r in previous),default='')
            older=[r for r in runs if r.get('id') not in original_ids and
                   (not oldest or (r.get('created_at') or '') <= oldest)]
            by_id={r.get('id'):r for r in runs}
            return [by_id.get(r.get('id'),r) for r in previous]+older
        atomic_json(output,runs)
        self.journal.record(key,output)
        return runs

    def _ci_run(self,slug,run):
        rid=run.get('id')
        if type(rid) is not int: return
        attempt=int(run.get('run_attempt') or 1)
        dest=self.cycle_dir/'repositories'/slug/'ci'/'runs'/f'{rid}.tar.gz.age'
        key=f'repo:{slug}:ci:{rid}'
        partial_marker=dest.with_name(dest.name+'.artifact-omissions.json')
        if self.journal.done(key):
            self.progress.emit('resume',f'CI {slug}:{rid} già archiviata; SHA-256 verificato',repository=slug)
            return 'partial' if partial_marker.is_file() else 'resumed'
        # Cache validity is tied to the CURRENT GitHub artifact inventory.
        with tempfile.TemporaryDirectory(dir=self.staging,prefix='.multi-backup-ci-preflight-') as tmp:
            pre=Path(tmp)/'artifacts.json'
            artifacts=self.backup.api(f'/repos/{slug}/actions/runs/{rid}/artifacts?per_page=100',
                                      pre,collection='artifacts',optional=False)
            if artifacts is None:
                raise RuntimeError('Cannot validate artifact inventory for '+str(rid))
        report=self.cleanup_reports.get(slug)
        if report and report.get('deleted_artifact_ids'):
            deleted=set(report['deleted_artifact_ids'])
            artifacts=[a for a in artifacts if a.get('id') not in deleted]
        artifact_view=[(a.get('id'),a.get('name'),a.get('expired'),a.get('updated_at')) for a in artifacts]
        cache_key=hashlib.sha256(json.dumps({
            'run':rid,'attempt':attempt,'updated':run.get('updated_at'),
            'artifacts':artifact_view,'include_logs':not self.args.skip_ci_logs,
            'include_artifacts':not self.args.skip_ci_artifacts,
            'recipient':self.args.recipient},sort_keys=True).encode()).hexdigest()[:28]
        cachedir=self.output/'.cache'/'ci'/slug
        cachedir.mkdir(parents=True,exist_ok=True,mode=0o700)
        cached=cachedir/(str(rid)+'-'+cache_key+'.tar.gz.age')
        ref=cached.with_suffix(cached.suffix+'.sha256')
        if self.args.recipient and cached.is_file() and ref.is_file() and sha256(cached)==ref.read_text('utf-8').split()[0]:
            dest.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
            try: os.link(cached,dest)
            except OSError: shutil.copy2(cached,dest)
            checksum_sidecar(dest)
            self.journal.record(key,dest)
            self.counts['ci_cache_reused']+=1
            return 'cached'
        omitted=[]
        def producer(root):
            rd=root/'repositories'/slug/'ci'/'runs'/str(rid)
            rd.mkdir(parents=True,exist_ok=True,mode=0o700)
            atomic_json(rd/'run.json',run)
            atomic_json(rd/'artifacts_metadata.json',artifacts)
            base=f'/repos/{slug}/actions/runs/{rid}'
            self.backup.api(base+'/jobs?per_page=100',rd/'jobs.json',collection='jobs')
            for old in range(1,attempt):
                sub=rd/'prior_attempts'/str(old)
                self.backup.api(base+f'/attempts/{old}/jobs?per_page=100',
                                sub/'jobs.json',collection='jobs')
            if not self.args.skip_ci_logs:
                for old in range(1,attempt+1):
                    filepath=rd/'logs.txt' if old==attempt else rd/'prior_attempts'/str(old)/'logs.txt'
                    filepath.parent.mkdir(parents=True,exist_ok=True)
                    with filepath.open('wb') as fp:
                        cmd=['run','view',str(rid),'--repo',slug,'--log']
                        if old!=attempt: cmd+=['--attempt',str(old)]
                        if self.backup.gh(cmd,f'CI logs {slug} {rid} attempt {old}',file=fp,timeout=1800) is None:
                            filepath.unlink(missing_ok=True)
            if not self.args.skip_ci_artifacts and any(a.get('expired') is False for a in artifacts):
                artdir=rd/'artifacts'; artdir.mkdir(parents=True,exist_ok=True)
                for item in artifacts:
                    if item.get('expired') is not False: continue
                    aid=item.get('id')
                    if type(aid) is not int: continue
                    try:
                        retrieve_artifact(
                            slug, rid, item, artifacts, artdir, self.backup.gh,
                            on_retry=lambda artifact_id, n, total:
                                self.progress.emit(
                                    'ci-retry',
                                    f'Artifact CI {artifact_id}: retry {n}/{total} '
                                    'prima di cercare un altra esecuzione CI',
                                    repository=slug))
                    except ArtifactDownloadError as exc:
                        if not self.args.ignore_ci_artifact_errors:
                            raise
                        failure={'id':aid,'name':str(item.get('name') or ''),
                                 'error':str(exc)}
                        omitted.append(failure)
                        self.progress.emit(
                            'ci-warning',
                            f'Artifact {aid} non disponibile: contenuto omesso '
                            'esplicitamente; cerco altre CI complete',
                            repository=slug)
                if omitted:
                    # The omission is part of the encrypted archive itself.
                    atomic_json(rd/'artifact_download_errors.json',omitted)
        self._archive(key,dest,producer)
        if omitted:
            atomic_json(partial_marker,{'run_id':rid,'artifacts':omitted,
                                        'complete':False})
            self.counts['ci_runs_partially_archived']=self.counts.get('ci_runs_partially_archived',0)+1
            return 'partial'
        if dest.is_file() and self.args.recipient:
            try: os.link(dest,cached)
            except FileExistsError: pass
            except OSError: shutil.copy2(dest,cached)
            checksum_sidecar(cached)
        self.counts['ci_runs_archived']+=1
        return 'new'

    def _ci(self,slug):
        runs=self._ci_index(slug)
        if not runs:
            self.progress.emit('ci','No workflow runs for '+slug,repository=slug)
            return
        limit=self.args.max_runs_per_repo
        target=min(limit,len(runs)) if limit else len(runs)
        workers=min(self.args.metadata_workers,4)
        if not self.args.recipient:
            workers=1
        valid=0
        examined=0
        problems=[]
        # Schedule only as many runs as needed to fill the requested number.
        # Every failed or partial run frees one slot for the next older run.
        candidates=iter(runs)
        pending={}
        with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as executor:
            def fill():
                while len(pending)<workers and valid+len(pending)<target:
                    item=next(candidates,None)
                    if item is None:
                        break
                    pending[executor.submit(self._ci_run,slug,item)]=item
            fill()
            while pending:
                finished,_=concurrent.futures.wait(
                    pending,return_when=concurrent.futures.FIRST_COMPLETED)
                for future in finished:
                    run=pending.pop(future)
                    examined+=1
                    rid=run.get('id')
                    try:
                        status=future.result()
                    except Exception as exc:
                        status='incomplete'
                        problems.append({'run_id':rid,'reason':str(exc)[:800]})
                        self.progress.emit(
                            'ci-warning',
                            f'CI {slug}:{rid} non scaricabile: {exc}; '
                            'tento la prossima esecuzione più vecchia, '
                            'senza interrompere gli altri repository.',
                            repository=slug)
                    if status=='partial':
                        problems.append({'run_id':rid,
                                         'reason':'Artifact omessi su richiesta esplicita'})
                    elif status in ('new','resumed','cached'):
                        valid+=1
                    self.progress.emit(
                        'ci',
                        f'{slug}: {valid}/{target} esecuzioni CI complete; '
                        f'{examined} verificate ({status})',
                        repository=slug,
                        sub_progress={'completed':valid,'total':target})
                fill()
        self.ci_issues.extend({'repository':slug,**issue} for issue in problems)
        self.counts['ci_candidates_skipped']=self.counts.get('ci_candidates_skipped',0)+len(problems)
        if problems:
            self.progress.emit(
                'ci-summary',
                f'{slug}: {valid}/{target} CI complete dopo '
                f'{examined} tentativi, {len(problems)} problematiche '
                '(ID e motivazioni nel manifest).',
                repository=slug)
        if valid<target:
            missing=target-valid
            self.ci_shortfalls[slug]=missing
            if not self.args.ignore_ci_artifact_errors:
                self.counts['errors']+=missing
                self.progress.emit(
                    'ci-shortfall',
                    f'{slug}: mancano {missing} esecuzioni CI complete; '
                    'checkpoint conservato, ripresa consigliata.',
                    repository=slug)
            else:
                self.progress.emit(
                    'ci-ignored',
                    f'{slug}: mancano {missing} CI complete; '
                    'opzione ignora errori attiva. '
                    'Il backup con omissioni sarà segnalato nei report.',
                    repository=slug)
        self.counts['ci_runs_fully_preserved']=self.counts.get(
            'ci_runs_fully_preserved',0)+valid

    def _metadata(self,repos):
        names=list(repos)
        total=self.overall_total
        for ix,slug in enumerate(names,1):
            repo=repos[slug]
            base=1+(ix-1)*5
            self.progress.emit('repo',f'{slug} ({ix}/{len(names)})',
                               completed=base,total=total,repository=slug)
            self._git(slug,repo)
            self.progress.emit('git',f'{slug}: mirror ready',completed=base+1,total=total,repository=slug)
            self._stage(slug,repo,'settings',lambda folder: self.backup.backup_settings(slug,folder,repo))
            self.progress.emit('settings',f'{slug}: settings saved',completed=base+2,total=total,repository=slug)
            self._stage(slug,repo,'releases',lambda folder: self.backup.backup_releases(slug,folder))
            self.progress.emit('releases',f'{slug}: releases saved',completed=base+3,total=total,repository=slug)
            self._stage(slug,repo,'issues',lambda folder: self.backup.backup_issues(slug,folder))
            self.progress.emit('issues',f'{slug}: issues saved',completed=base+4,total=total,repository=slug)
            self._ci(slug)
            self.progress.emit('repo',f'{slug}: complete',completed=base+5,total=total,repository=slug)

    def _common(self,repos):
        orgs=sorted({r['owner']['login'] for r in repos.values()
                     if r.get('owner',{}).get('type')=='Organization'})
        if not self.args.skip_org_settings:
            for org in orgs:
                self._archive('org:'+org,self.cycle_dir/'common'/'organizations'/f'{org}.tar.gz.age',
                              lambda root, org=org: self.backup.backup_organization(org))
        if not self.args.skip_gists:
            self._archive('gists',self.cycle_dir/'common'/'gists.tar.gz.age',
                          lambda root:self.backup.backup_gists())
        self._credentials_stream()

    def _credentials_stream(self):
        """Encrypt credentials directly, never writing secret values to staging."""
        dest=self.cycle_dir/'common'/'credentials.tar.gz.age'
        key='credentials'
        if self.journal.done(key):
            self.progress.emit('resume','Verified credentials archive')
            return
        dest.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
        if dest.is_file():
            sidecar=dest.with_suffix(dest.suffix+'.sha256')
            if sidecar.is_file() and sidecar.read_text('utf-8').split()[0]==sha256(dest):
                self.journal.record(key,dest)
                return
            raise RuntimeError('Unindexed credentials output; use --restart (the old file is preserved)')
        partial=dest.with_name(dest.name+'.partial')
        partial.unlink(missing_ok=True)
        try:
            with StreamingArchive(partial,self.args.recipient) as archive:
                self.backup.backup_credentials(archive)
            os.replace(partial,dest)
            checksum_sidecar(dest)
            self.journal.record(key,dest)
        finally:
            partial.unlink(missing_ok=True)

    def _manifest(self,repos,migrations):
        data={'tool_version':VERSION,'cycle':self.journal.cycle,
              'created_utc':dt.datetime.now(dt.timezone.utc).isoformat(),
              'repositories':list(repos), 'git_snapshots':self.repository_snapshots,
              'cleanup_reports':self.cleanup_reports,'counts':self.counts,
              'legacy_migrations':[m['source_sha256'] for m in migrations],
              'integrity':'Each output has SHA-256 sidecar; see .state/cycle-*.json',
              'ci_download_issues':self.ci_issues,
              'ci_run_shortfalls':self.ci_shortfalls,
              'ci_artifact_errors_ignored':self.args.ignore_ci_artifact_errors,
              'not_exportable':'GitHub secrets values and external registry contents'}
        self._archive('manifest',self.cycle_dir/'common'/'manifest.tar.gz.age',
                      lambda root:atomic_json(root/'backup_manifest.json',data))
        return data

    def _prune(self):
        # Only finished cycles with durable completion records are eligible.
        completed=[]
        for manifest in (self.output/'.state').glob('cycle-*.json'):
            try:
                data=json.loads(manifest.read_text('utf-8'))
                if data.get('finished'):
                    dest=self.output/'metadata'/'cycles'/data['cycle']
                    if dest.is_dir(): completed.append((data['cycle'],dest))
            except (ValueError,KeyError): continue
        completed.sort(reverse=True)
        keep=1 if self.args.purge_obsolete_metadata else self.args.max_metadata_backups
        for name,folder in completed[keep:]:
            if name != self.journal.cycle:
                shutil.rmtree(folder)
        # Migration source archives are immutable: they are never deleted here.

    def run(self):
        migrations=migrate_legacy(self.output,self.staging,self.args.identity,
                                  self.args.recipient,self.progress)
        repos=self._account()
        self.counts['repositories']=len(repos)
        orgs={r['owner']['login'] for r in repos.values()
              if r.get('owner',{}).get('type')=='Organization'}
        extras=(len(orgs) if not self.args.skip_org_settings else 0)+(
            0 if self.args.skip_gists else 1)+2
        self.overall_total=1+len(repos)*5+extras
        self.progress.emit('account','Repository discovery complete',completed=1,total=self.overall_total)
        if self.journal.resumed:
            self.progress.emit(
                'resume-summary',
                f"RIPRESA: {len(self.journal.data['steps'])} checkpoint registrati; "
                f"{len(repos)} repository rilevati; verifica e completamento "
                'delle unità mancanti in corso. La percentuale rappresenta '
                'le fasi di lavoro, non i byte salvati.',
                completed=1,total=self.overall_total)
        self.progress.emit('account','Starting repository snapshots',completed=1,total=self.overall_total)
        self._metadata(repos)
        self._common(repos)
        self.counts['errors'] += sum(bool(e.get('critical')) for e in self.backup.errors)
        # Never DELETE remotely before a verified backup of those Actions runs.
        if self.counts['errors']==0 and not self.ci_issues:
            self._cleanup(repos)
        elif self.args.cleanup_ci_artifacts:
            self.progress.emit('cleanup-skipped','Backup incomplete; refusing CI cleanup')
        if self.counts['errors']:
            atomic_json(self.output/'.state'/'last-incomplete.json',
                        {'cycle':self.journal.cycle,'counts':self.counts,
                         'reason':'Some required export items failed; retry the same command'})
            self.progress.emit(
                'incomplete',f"{self.counts['errors']} errori: backup parziale, "
                'checkpoint preservato; riprendere con gli stessi parametri. '
                'Nessuna pulizia remota non verificata è stata eseguita.')
            return 2
        self._manifest(repos,migrations)
        self.journal.finish()
        (self.output/'.state'/'last-incomplete.json').unlink(missing_ok=True)
        self._prune()
        state='complete-with-warnings' if self.ci_issues else 'complete'
        label=('Backup con omissioni esplicite' if self.ci_shortfalls else
               'Backup completato; alcune CI non disponibili sostituite')
        if state=='complete':
            label='Backup completo'
        self.progress.emit(
            state,f'{label}: ciclo {self.journal.cycle}',
            completed=self.overall_total,total=self.overall_total,
            summary={**self.counts,'ci_download_issues':self.ci_issues,
                     'ci_run_shortfalls':self.ci_shortfalls})
        return 0


def build_parser():
    p=argparse.ArgumentParser(prog='multi-backup',description='Resumable encrypted GitHub backup and archive browser')
    p.add_argument('--version',action='version',version=VERSION)
    sub=p.add_subparsers(dest='command')
    b=sub.add_parser('backup',help='Create or resume a backup cycle')
    b.add_argument('--output-dir',type=Path,default=Path.home()/'Multi-Backups')
    b.add_argument('--backend', choices=['github','dropbox'],default='github',help='Backup source plugin (default github)')
    b.add_argument('--dropbox-path',default='',help='Dropbox folder path (Dropbox backend only)')
    b.add_argument('--github-user',help='GitHub login to back up (must match authenticated account)')
    b.add_argument('--staging-dir',type=Path,help='Private encrypted-volume staging directory')
    b.add_argument('--identity',type=Path,help='Age private key for migrating legacy metadata')
    b.add_argument('--recipient',help='Age public recipient key (recommended)')
    b.add_argument('--credentials-dir',type=Path)
    b.add_argument('--include-gh-token',action='store_true')
    b.add_argument('--max-copies-per-repo',type=int,default=5)
    b.add_argument('--max-metadata-backups',type=int,default=5)
    b.add_argument('--max-runs-per-repo',type=int,default=5,help='Target complete CI runs/repo (default 5, backfill older runs; 0=all)')
    b.add_argument('--ignore-ci-artifact-errors',action='store_true',help='Opt in to preserving partial CI runs when artifact downloads fail; log omissions and continue')
    b.add_argument('--cleanup-ci-artifacts',action='store_true',help='Opt in to deleting eligible Actions artifacts')
    b.add_argument('--keep-ci-runs',type=int,default=5)
    b.add_argument('--cleanup-ci-dry-run',action='store_true',help='Only simulate Actions deletion; still back up')
    b.add_argument('--dry-run',action='store_true',help='Preview, do not back up or delete GitHub artifacts')
    b.add_argument('--resume',action='store_true',help='Require a matching incomplete cycle, preserving its backup name; error if none exists')
    b.add_argument('--restart',action='store_true',help='Explicit Start: new cycle name, preserving unfinished checkpoints and archives')
    b.add_argument('--purge-obsolete-metadata',action='store_true',help='Prune older per-repo metadata only after success')
    b.add_argument('--metadata-workers',type=int,default=4,help='Parallel GitHub API/download workers (default 4)')
    b.add_argument('--progress',action='store_true',help='Show terminal progress bars')
    b.add_argument('--json-progress',action='store_true',help='Output newline-delimited progress JSON to stderr')
    for field in ('ci-logs','ci-artifacts','release-assets','issue-details','issue-events','org-settings','gists'):
        b.add_argument('--skip-'+field,action='store_true')
    l=sub.add_parser('list',help='List saved repository and metadata archives')
    l.add_argument('--output-dir',type=Path,default=Path.home()/'Multi-Backups')
    l.add_argument('--json',action='store_true')
    v=sub.add_parser('view',help='List members of one encrypted archive')
    v.add_argument('archive',type=Path); v.add_argument('--identity',type=Path,required=True)
    s=sub.add_parser('show',help='Display a small text/JSON member of an archive')
    s.add_argument('archive',type=Path);s.add_argument('member');s.add_argument('--identity',type=Path,required=True)
    e=sub.add_parser('extract',help='Safely extract a specific encrypted archive')
    e.add_argument('archive',type=Path);e.add_argument('destination',type=Path);e.add_argument('--identity',type=Path,required=True)
    c=sub.add_parser('checkout',help='Checkout files from a saved bare Git mirror')
    c.add_argument('archive',type=Path);c.add_argument('destination',type=Path);c.add_argument('--identity',type=Path,required=True)
    m=sub.add_parser('mount',help='Mount extracted files using bindfs or Windows subst')
    m.add_argument('archive',type=Path);m.add_argument('mountpoint');m.add_argument('--identity',type=Path,required=True)
    m.add_argument('--output-dir',type=Path,default=Path.home()/'Multi-Backups')
    u=sub.add_parser('unmount',help='Unmount and remove the temporary extracted files')
    u.add_argument('mountpoint');u.add_argument('--output-dir',type=Path,default=Path.home()/'Multi-Backups')
    browse=sub.add_parser('browse',help='Interactive backup catalog and archive viewer')
    browse.add_argument('--output-dir',type=Path,default=Path.home()/'Multi-Backups')
    browse.add_argument('--identity',type=Path,required=True)
    sub.add_parser('plugins',help='List available backend plugins and activation status')
    plug=sub.add_parser('plugin',help='Enable or disable a backend plugin')
    plug.add_argument('action',choices=['enable','disable'])
    plug.add_argument('backend',choices=['github','dropbox'])
    sett=sub.add_parser('settings',help='Inspect or change application settings')
    sett.add_argument('action',choices=['show','channel','auto-check'])
    sett.add_argument('value',nargs='?')
    keys=sub.add_parser('keys',help='Generate and inspect age encryption keys')
    keys.add_argument('action',choices=['generate','public'])
    keys.add_argument('--name',default='default')
    updates=sub.add_parser('updates',help='Check and stage a trusted native release')
    updates.add_argument('action',choices=['check','download'])
    updates.add_argument('--channel',choices=['stable','early-release'])
    updates.add_argument('--download-dir',type=Path,default=Path.home()/'Downloads')
    sub.add_parser('gui',help='Open the graphical interface')
    return p


def browse_interactive(args):
    entries=catalog(args.output_dir)
    if not entries:
        print('No saved backups found')
        return 0
    for idx,entry in enumerate(entries,1):
        print(f'{idx:3}. [{entry["kind"]:8}] {entry["relative"]} ({entry["size"]:,} bytes)')
    raw=input('Select archive number (empty to cancel): ').strip()
    if not raw: return 0
    if not raw.isdigit() or not 1<=int(raw)<=len(entries): raise ValueError('Invalid selection')
    target=Path(entries[int(raw)-1]['path'])
    choice=input('Action [list/show/extract/checkout/mount]: ').strip().lower()
    if choice=='list':
        for member in list_members(target,args.identity):print(member['path'])
    elif choice=='show': print(read_member(target,args.identity,input('Member path: ').strip()))
    elif choice in {'extract','checkout'}:
        dest=Path(input('Destination path: ').strip())
        if choice=='extract':extract_archive(target,args.identity,dest)
        else:checkout_git(target,args.identity,dest)
    elif choice=='mount':
        point=input('Mount point (Windows drive letter e.g. X:): ').strip()
        print('Mounted using workspace',mount_archive(target,args.identity,point,args.output_dir/'.state'))
    elif choice:raise ValueError('Unknown action')
    return 0


def main(argv=None):
    from runtime_bootstrap import configure_path
    configure_path()
    argv=list(sys.argv[1:] if argv is None else argv)
    if not argv:
        # Display the top-level command reference rather than starting a backup.
        argv=['--help']
    elif argv[0].startswith('-') and argv[0] not in ('--version','--help','-h'):
        # Backwards-compatible shorthand for old github-backup flags.
        argv.insert(0,'backup')
    args=build_parser().parse_args(argv)
    if args.command in ('plugins','plugin','settings','keys','updates'):
        from settings import load_settings,save_settings,set_backend_enabled
        config=load_settings()
        if args.command=='plugins':
            from backends import available
            for name,backend in available().items():
                print(f'{name:10} {"enabled" if name in config["enabled_backends"] else "disabled":9} '
                      f'{backend.description}')
        elif args.command=='plugin':
            config=set_backend_enabled(args.backend,args.action=='enable')
            print(json.dumps({'backend':args.backend,'enabled':args.backend in config['enabled_backends']}))
        elif args.command=='settings':
            if args.action=='show':print(json.dumps(config,indent=2))
            elif args.action=='channel':
                if args.value not in ('stable','early-release'):raise ValueError('Specify stable or early-release')
                config['update_channel']=args.value;save_settings(config)
                print('Update channel:',args.value)
            else:
                if args.value not in ('on','off'):raise ValueError('Specify on or off')
                config['auto_update_check']=args.value=='on';save_settings(config)
                print('Automatic update checks:',args.value)
        elif args.command=='keys':
            from key_management import generate_key,public_recipient
            if args.action=='generate': print(json.dumps(generate_key(args.name),indent=2))
            else:print(public_recipient(args.name))
        elif args.command=='updates':
            from updater import update_info, verified_asset_from_release,download_asset
            info=update_info(args.channel or config['update_channel'])
            if args.action=='check':
                info['asset']={k:info['asset'].get(k) for k in ('name','size','digest')} if info['asset'] else None
                print(json.dumps(info,indent=2))
            else:
                asset,digest=verified_asset_from_release(info)
                print('Verified download:',download_asset(asset,args.download_dir,digest))
                print('Review the installer and OS signature before installation.')
        return 0
    if args.command=='gui':
        from gui import run_gui
        return run_gui()
    if args.command=='list':
        entries=catalog(args.output_dir)
        if args.json:print(json.dumps(entries,indent=2))
        else:
            for entry in entries:print(f'{entry["kind"]:8} {entry["relative"]} ({entry["size"]:,} bytes)')
        return 0
    if args.command=='view':
        for item in list_members(args.archive,args.identity):print(f'{item["bytes"]:>12} {item["path"]}')
        return 0
    if args.command=='show':print(read_member(args.archive,args.identity,args.member));return 0
    if args.command=='extract':print(extract_archive(args.archive,args.identity,args.destination));return 0
    if args.command=='checkout':print(checkout_git(args.archive,args.identity,args.destination));return 0
    if args.command=='mount':
        print('Mounted; staging:',mount_archive(args.archive,args.identity,args.mountpoint,args.output_dir/'.state'));return 0
    if args.command=='unmount':unmount_archive(args.output_dir/'.state',args.mountpoint);print('Unmounted');return 0
    if args.command=='browse':return browse_interactive(args)
    if args.command!='backup':raise ValueError('Unsupported command')
    if args.backend=='dropbox' and (args.cleanup_ci_artifacts or args.purge_obsolete_metadata or args.cleanup_ci_dry_run):
        raise ValueError('Dropbox never supports source cleanup or purge')
    if args.github_user and not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9-]{0,38}',args.github_user):
        raise ValueError('Invalid GitHub username')
    if args.backend=='dropbox' and args.github_user:
        raise ValueError('--github-user is for the GitHub backend only')
    if args.keep_ci_runs<1 or args.metadata_workers<1 or args.metadata_workers>16:
        raise ValueError('keep-ci-runs must be >= 1; metadata-workers must be in [1,16]')
    if args.max_copies_per_repo<1 or args.max_metadata_backups<1 or args.max_runs_per_repo<0:
        raise ValueError('Invalid retention limit')
    if args.restart and args.resume:raise ValueError('Choose --restart or --resume, not both')
    if args.cleanup_ci_dry_run and not args.cleanup_ci_artifacts:
        raise ValueError('--cleanup-ci-dry-run requires --cleanup-ci-artifacts')
    if args.backend == 'dropbox' and args.dry_run:
        print(json.dumps({'backend':'dropbox','mode':'dry-run','message':'No files read or modified.'}))
        return 0
    if args.dry_run:
        print(json.dumps({'mode':'dry-run','cleanup_enabled':args.cleanup_ci_artifacts,
                          'keep_ci_runs':args.keep_ci_runs,'output_dir':str(args.output_dir),
                          'message':'No artifact will be deleted or backup written.'},indent=2))
        if args.cleanup_ci_artifacts:
            # Preflight is read-only: enumerate repositories and produce a per-repo plan.
            probe=Backup(args,Path(tempfile.mkdtemp(prefix='multi-backup-dry-run-')))
            try:
                repos=probe.get_repos()
                totals={'repositories':0,'eligible':0,'protected':0}
                for slug,repo in repos.items():
                    report=GitHubCleanup(args.keep_ci_runs,dry_run=True).run(repo)
                    c=report.get('counts',{})
                    totals['repositories']+=1; totals['eligible']+=c.get('eligible_artifacts',0)
                    totals['protected']+=c.get('protected_artifacts',0)
                    print(f"{slug}: would_delete={c.get('eligible_artifacts',0)} "
                          f"protected={c.get('protected_artifacts',0)} "
                          f"error={report.get('error','none')}")
                print('DRY-RUN TOTAL:',json.dumps(totals))
            finally:shutil.rmtree(probe.root,ignore_errors=True)
        return 0
    from backends import available
    from settings import load_settings
    configured=load_settings()
    if args.backend not in configured['enabled_backends']:
        raise RuntimeError(f'Backend {args.backend} is disabled. Use: multi-backup plugin enable {args.backend}')
    return available()[args.backend].handler(args)

if __name__=='__main__':
    try:sys.exit(main())
    except KeyboardInterrupt:
        print('Interrupted. Resume using the same command.',file=sys.stderr)
        sys.exit(130)
    except (ArchiveError,RuntimeError,ValueError,OSError) as exc:
        print('ERROR: '+str(exc),file=sys.stderr)
        sys.exit(1)
