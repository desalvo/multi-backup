#!/usr/bin/env python3
"""Create an encrypted, best-effort disaster-recovery archive for GitHub.

Requires: Python 3.9+, gh, git, git-lfs, age, tar.
By default does not modify GitHub. Optional artifact cleanup deletes only
GitHub Actions artifacts, subject to conservative preflight safety checks.
"""
from __future__ import annotations

import argparse
import io
import tarfile
import datetime as dt
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from urllib.parse import quote

from ci_artifact_cleanup import GitHubCleanup

VERSION = "1.3.0"
SLUG_RE = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")


def stamp():
    return dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def save_json(path: Path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def run_cmd(cmd, *, stdout=None, timeout=None):
    return subprocess.run(cmd, stdout=stdout if stdout is not None else subprocess.PIPE,
                          stderr=subprocess.PIPE, check=False, timeout=timeout)


class Backup:
    def __init__(self, args, root: Path):
        self.args = args
        self.root = root
        self.errors = []
        self.counts = {"repositories": 0, "mirrors_ok": 0, "mirrors_failed": 0,
                       "wiki_ok": 0, "releases": 0, "ci_runs": 0,
                       "ci_log_downloads": 0, "ci_artifact_runs": 0,
                       "gists": 0, "git_snapshots_new": 0,
                       "git_snapshots_deduplicated": 0}
        self.username = ""
        self.secret_inventory = []
        self.repository_snapshots = {}
        self.retention_actions = []
        self.ci_cleanup_reports = {}

    def problem(self, name, detail, critical=False):
        self.errors.append({"time": stamp(), "item": name, "critical": critical,
                            "reason": str(detail)[:1600]})
        print(f"  {'ERRORE' if critical else 'AVVISO'}: {name}", file=sys.stderr, flush=True)

    def gh(self, args, label, *, optional=True, file=None, timeout=None):
        try:
            p = run_cmd(["gh", *args], stdout=file, timeout=timeout)
        except (OSError, subprocess.TimeoutExpired) as ex:
            self.problem(label, ex, critical=not optional)
            return None
        if p.returncode != 0:
            self.problem(label, p.stderr.decode("utf-8", "replace"), critical=not optional)
            return None
        return p.stdout if file is None else b""

    def api(self, endpoint, destination: Path, *, collection=None, optional=True):
        """Save JSON API result. collection indicates a paginated list property."""
        args = ["api"]
        if collection is not None:
            args += ["--paginate", "--slurp"]
        args.append(endpoint)
        raw = self.gh(args, "API " + endpoint, optional=optional, timeout=1200)
        if raw is None:
            return None
        try:
            obj = json.loads(raw)
            if collection is not None:
                pages = obj if isinstance(obj, list) else [obj]
                if collection == "":
                    obj = [entry for page in pages for entry in page]
                else:
                    if any(not isinstance(page, dict) for page in pages):
                        raise TypeError(f"Pagine API inattese per {endpoint}, attesa chiave {collection}")
                    obj = [entry for page in pages for entry in page.get(collection, [])]
            save_json(destination, obj)
            if destination.name.endswith("secrets_METADATA_ONLY.json") and isinstance(obj, list):
                for secret in obj:
                    if isinstance(secret, dict) and secret.get("name"):
                        self.secret_inventory.append({
                            "source": str(destination.relative_to(self.root)),
                            "name": secret["name"],
                            "value_exported_by_api": False,
                        })
            return obj
        except (ValueError, TypeError, KeyError) as ex:
            self.problem("JSON " + endpoint, ex, critical=not optional)
            return None

    def executable(self, cmd, label, *, optional=True, file=None, timeout=None):
        try:
            p = run_cmd(cmd, stdout=file, timeout=timeout)
        except (OSError, subprocess.TimeoutExpired) as ex:
            self.problem(label, ex, critical=not optional)
            return False
        if p.returncode != 0:
            self.problem(label, p.stderr.decode("utf-8", "replace"), critical=not optional)
            return False
        return True

    def backup_account(self):
        print("[Account] Profilo e configurazioni esportabili", flush=True)
        folder = self.root / "account"
        me = self.api("/user", folder / "profile.json", optional=False)
        if not isinstance(me, dict) or not me.get("login"):
            raise RuntimeError("Impossibile leggere /user: verifica gh auth login")
        self.username = me["login"]
        chosen=getattr(self.args,'github_user',None)
        if chosen and chosen.lower()!=self.username.lower():
            raise RuntimeError('GitHub login mismatch: authenticated as '+self.username+
                               ', requested '+chosen+'. Sign in with gh auth login for the requested account.')
        for ep, filename, coll in [
            ("/user/emails?per_page=100", "emails.json", ""),
            ("/user/keys?per_page=100", "ssh_public_keys.json", ""),
            ("/user/gpg_keys?per_page=100", "gpg_public_keys.json", ""),
            ("/user/ssh_signing_keys?per_page=100", "ssh_signing_public_keys.json", ""),
            ("/user/orgs?per_page=100", "organizations.json", ""),
            ("/user/memberships/orgs?per_page=100", "organization_memberships.json", ""),
        ]:
            self.api(ep, folder / filename, collection=coll)
        self.api("/rate_limit", folder / "rate_limit_initial.json")
        return me

    def get_repos(self):
        repos = self.api(
            "/user/repos?affiliation=owner,collaborator,organization_member&per_page=100",
            self.root / "account" / "repositories.json", collection="", optional=False)
        if repos is None:
            raise RuntimeError("Elenco dei repository non disponibile")
        valid = []
        for r in repos:
            slug = r.get("full_name", "")
            if SLUG_RE.fullmatch(slug) and all(x not in {".", ".."} for x in slug.split("/")):
                valid.append(r)
            else:
                self.problem("repository non valido", repr(slug), critical=True)
        chosen=getattr(self.args,'github_user',None)
        if chosen:
            valid=[r for r in valid if r.get('owner',{}).get('login','').lower()==chosen.lower()]
            # Do not save other accounts' repository inventory in the selected backup.
            save_json(self.root / 'account' / 'repositories.json',valid)
        return {r["full_name"]: r for r in valid}

    def backup_organization(self, org):
        print(f"[Organization] {org}", flush=True)
        folder = self.root / "organizations" / org
        endpoints = [
            (f"/orgs/{org}", "settings.json", None),
            (f"/orgs/{org}/actions/permissions", "actions_permissions.json", None),
            (f"/orgs/{org}/actions/permissions/workflow", "actions_workflow_permissions.json", None),
            (f"/orgs/{org}/actions/permissions/artifact-and-log-retention", "actions_retention.json", None),
            (f"/orgs/{org}/actions/secrets?per_page=100", "actions_secrets_METADATA_ONLY.json", "secrets"),
            (f"/orgs/{org}/actions/variables?per_page=100", "actions_variables.json", "variables"),
            (f"/orgs/{org}/actions/runners?per_page=100", "runners_METADATA_ONLY.json", "runners"),
            (f"/orgs/{org}/actions/runner-groups?per_page=100", "runner_groups.json", "runner_groups"),
            (f"/orgs/{org}/hooks?per_page=100", "webhooks.json", ""),
            (f"/orgs/{org}/rulesets?per_page=100", "rulesets.json", ""),
            (f"/orgs/{org}/teams?per_page=100", "teams.json", ""),
        ]
        results = {}
        with ThreadPoolExecutor(max_workers=getattr(self.args, "metadata_workers", 4)) as executor:
            collected = list(executor.map(
                lambda spec: (spec[1], self.api(spec[0], folder / spec[1], collection=spec[2])),
                endpoints))
        results.update(collected)
        for secret in results.get("actions_secrets_METADATA_ONLY.json") or []:
            if secret.get("visibility") == "selected" and secret.get("name"):
                name = quote(secret["name"], safe="")
                self.api(f"/orgs/{org}/actions/secrets/{name}/repositories?per_page=100",
                         folder / "secret_repo_access" / f"{name}.json", collection="repositories")
        for var in results.get("actions_variables.json") or []:
            if var.get("visibility") == "selected" and var.get("name"):
                name = quote(var["name"], safe="")
                self.api(f"/orgs/{org}/actions/variables/{name}/repositories?per_page=100",
                         folder / "variable_repo_access" / f"{name}.json", collection="repositories")
        for ruleset in results.get("rulesets.json") or []:
            if ruleset.get("id"):
                self.api(f"/orgs/{org}/rulesets/{ruleset['id']}",
                         folder / "ruleset_details" / f"{ruleset['id']}.json")
        for team in results.get("teams.json") or []:
            if team.get("slug"):
                team_slug = quote(team["slug"], safe="")
                self.api(f"/orgs/{org}/teams/{team_slug}",
                         folder / "team_details" / team_slug / "team.json")
                self.api(f"/orgs/{org}/teams/{team_slug}/repos?per_page=100",
                         folder / "team_details" / team_slug / "repositories.json", collection="")
                self.api(f"/orgs/{org}/teams/{team_slug}/members?per_page=100",
                         folder / "team_details" / team_slug / "members.json", collection="")

    def backup_settings(self, slug, folder, repo):
        sdir = folder / "settings"
        base = f"/repos/{slug}"
        endpoints = [
            (base, "repository.json", None),
            (base + "/actions/permissions", "actions_permissions.json", None),
            (base + "/actions/permissions/workflow", "actions_workflow_permissions.json", None),
            (base + "/actions/permissions/access", "actions_access.json", None),
            (base + "/actions/permissions/artifact-and-log-retention", "actions_retention.json", None),
            (base + "/actions/workflows?per_page=100", "workflows.json", "workflows"),
            (base + "/actions/secrets?per_page=100", "actions_secrets_METADATA_ONLY.json", "secrets"),
            (base + "/actions/variables?per_page=100", "actions_variables.json", "variables"),
            (base + "/actions/organization-secrets?per_page=100", "shared_org_secrets_METADATA_ONLY.json", "secrets"),
            (base + "/actions/organization-variables?per_page=100", "shared_org_variables.json", "variables"),
            (base + "/dependabot/secrets?per_page=100", "dependabot_secrets_METADATA_ONLY.json", "secrets"),
            (base + "/codespaces/secrets?per_page=100", "codespaces_secrets_METADATA_ONLY.json", "secrets"),
            (base + "/actions/runners?per_page=100", "runners_METADATA_ONLY.json", "runners"),
            (base + "/environments?per_page=100", "environments.json", "environments"),
            (base + "/rulesets?per_page=100", "rulesets.json", ""),
            (base + "/hooks?per_page=100", "webhooks.json", ""),
            (base + "/keys?per_page=100", "deploy_public_keys.json", ""),
            (base + "/collaborators?per_page=100", "collaborators.json", ""),
            (base + "/teams?per_page=100", "teams.json", ""),
            (base + "/branches?per_page=100", "branches.json", ""),
            (base + "/labels?per_page=100", "labels.json", ""),
            (base + "/milestones?state=all&per_page=100", "milestones.json", ""),
            (base + "/pages", "github_pages.json", None),
            (base + "/deployments?per_page=100", "deployments.json", ""),
        ]
        results = {}
        with ThreadPoolExecutor(max_workers=getattr(self.args, "metadata_workers", 4)) as executor:
            collected = list(executor.map(
                lambda spec: (spec[1], self.api(spec[0], sdir / spec[1], collection=spec[2])),
                endpoints))
        results.update(collected)
        for env in results.get("environments.json") or []:
            name = env.get("name", "")
            enc = quote(name, safe="")
            envdir = sdir / "environment_details" / str(env["id"])
            self.api(f"{base}/environments/{enc}", envdir / "environment.json")
            self.api(f"{base}/environments/{enc}/secrets?per_page=100",
                     envdir / "secrets_METADATA_ONLY.json", collection="secrets")
            self.api(f"{base}/environments/{enc}/variables?per_page=100",
                     envdir / "variables.json", collection="variables")
            self.api(f"{base}/environments/{enc}/deployment-branch-policies?per_page=100",
                     envdir / "branch_policies.json", collection="branch_policies")
        for branch in results.get("branches.json") or []:
            if branch.get("protected"):
                name = branch.get("name", "")
                safe = quote(name, safe="")
                # Branches can have slashes: numeric-indexed filenames avoid path traversal.
                ident = hashlib.sha256(name.encode()).hexdigest()[:16]
                self.api(f"{base}/branches/{safe}/protection",
                         sdir / "branch_protection" / f"{ident}.json")
        for rule in results.get("rulesets.json") or []:
            if "id" in rule:
                self.api(f"{base}/rulesets/{rule['id']}",
                         sdir / "ruleset_details" / f"{rule['id']}.json")

    @staticmethod
    def _git_text(mirror: Path, *args):
        res = run_cmd(["git", "-C", str(mirror), *args], timeout=90)
        if res.returncode:
            raise RuntimeError(res.stderr.decode("utf-8", "replace")[:1000])
        return res.stdout.decode("utf-8", "replace").strip()

    @staticmethod
    def _main_info(mirror: Path, repo):
        # Prefer main explicitly; if absent, correctly label the default branch.
        branch = "main"
        try:
            commit = Backup._git_text(mirror, "rev-parse", "--verify", "refs/heads/main^{commit}")
        except Exception:
            branch = repo.get("default_branch") or "main"
            try:
                commit = Backup._git_text(mirror, "rev-parse", "--verify", f"refs/heads/{branch}^{{commit}}")
            except Exception:
                return branch, "unborn", "undated"
        date_raw = Backup._git_text(mirror, "show", "-s", "--format=%cI", commit)
        # Always use the COMMIT DATE on main, not the download date.
        date = dt.datetime.fromisoformat(date_raw.replace("Z", "+00:00"))
        return branch, commit, date.astimezone(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")

    @staticmethod
    def _refs_fingerprint(mirror: Path, wiki: Path | None):
        h = hashlib.sha256()
        for label, path in (("repository", mirror), ("wiki", wiki)):
            h.update((label + "\0").encode())
            if path is None or not path.exists():
                h.update(b"<absent>\n")
                continue
            # Git content-addressed refs identify all reachable objects, regardless
            # of packfile repacking, clone date and file modification timestamps.
            refs = Backup._git_text(path, "for-each-ref", "--format=%(refname) %(objectname)")
            h.update((refs + "\n").encode("utf-8"))
            # Empty repos can differ in symbolic HEAD even when no refs exist.
            try:
                head = Backup._git_text(path, "symbolic-ref", "HEAD")
            except Exception:
                head = "<detached>"
            h.update((head + "\n").encode())
        return h.hexdigest()

    @staticmethod
    def _checksum(path: Path):
        h = hashlib.sha256()
        with path.open("rb") as stream:
            for chunk in iter(lambda: stream.read(8 * 1024 * 1024), b""):
                h.update(chunk)
        return h.hexdigest()

    def _existing_snapshots(self, repo_dir: Path):
        result = []
        for sidecar in repo_dir.glob("*__git-*.tar.gz.age.json"):
            try:
                record = json.loads(sidecar.read_text(encoding="utf-8"))
                archive = repo_dir / record["filename"]
                if archive.is_file() and archive.parent == repo_dir:
                    result.append((record, archive, sidecar))
            except (ValueError, OSError, KeyError) as exc:
                self.problem(f"Indice backup invalido: {sidecar}", str(exc), critical=True)
        return result

    def _retain_git_snapshots(self, slug, repo_dir: Path):
        limit = self.args.max_copies_per_repo
        existing = self._existing_snapshots(repo_dir)
        # Oldest seen state first, newest last. One copy per distinct fingerprint.
        existing.sort(key=lambda entry: (entry[0].get("last_seen_utc") or
                                         entry[0].get("created_utc", ""),
                                         entry[0].get("created_utc", "")))
        for record, archive, index in existing[:-limit]:
            checksum_file = archive.with_suffix(archive.suffix + ".sha256")
            # Delete only fully indexed/valid snapshots, never unrecognized files.
            archive.unlink()
            index.unlink()
            checksum_file.unlink(missing_ok=True)
            self.retention_actions.append({"repo": slug, "deleted": record["filename"],
                                           "reason": f"oltre {limit} snapshot Git distinti"})
            print(f"  [Retention] Rimosso snapshot Git più vecchio: {archive.name}", flush=True)

    def backup_git_snapshot(self, slug, folder, repo, repo_dir: Path):
        mirror = folder / "git" / (repo["name"] + ".git")
        mirror.parent.mkdir(parents=True, exist_ok=True)
        print("  Git mirror + LFS + wiki", flush=True)
        if not self.executable(["git", "clone", "--mirror", "--quiet",
                                f"https://github.com/{slug}.git", str(mirror)],
                               slug + " git clone", optional=False, timeout=3600):
            self.counts["mirrors_failed"] += 1
            return None
        self.counts["mirrors_ok"] += 1
        healthy = True
        if not self.executable(["git", "-C", str(mirror), "lfs", "fetch", "--all"],
                               slug + " git lfs fetch --all", optional=False, timeout=3600):
            healthy = False
        if not self.executable(["git", "-C", str(mirror), "fsck", "--full", "--no-reflogs"],
                               slug + " git fsck", optional=False, timeout=3600):
            healthy = False
        wiki = None
        if repo.get("has_wiki"):
            wiki = folder / "git" / (repo["name"] + ".wiki.git")
            if self.executable(["git", "clone", "--mirror", "--quiet",
                                f"https://github.com/{slug}.wiki.git", str(wiki)],
                               slug + " wiki (puo' essere non inizializzata)", timeout=600):
                self.counts["wiki_ok"] += 1
            else:
                # A GitHub wiki can be enabled but never initialized. A warning
                # is retained; its absence is part of the fingerprint.
                wiki = None
        try:
            branch, main_sha, main_date = self._main_info(mirror, repo)
            fingerprint = self._refs_fingerprint(mirror, wiki)
        except Exception as exc:
            self.problem(f"Impossibile identificare snapshot {slug}", exc, critical=True)
            return None
        if not healthy:
            # Do not silently label a partially fetched LFS mirror as an identical,
            # complete backup. Retain the prior valid snapshot and mark failure.
            self.problem(f"Snapshot {slug} incompleto",
                         "git fsck o git lfs fetch fallito: snapshot non sostituito", critical=True)
            return None

        repo_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
        for record, existing, sidecar in self._existing_snapshots(repo_dir):
            if record.get("fingerprint") == fingerprint:
                # Full integrity check is necessary: cannot deduplicate onto a
                # corrupt encrypted backup merely because the filename matches.
                digest = self._checksum(existing)
                if digest != record.get("archive_sha256"):
                    self.problem(f"Archivio esistente corrotto {existing}",
                                 "SHA-256 diverso: mantengo il file da ispezionare", critical=True)
                    continue
                record["last_seen_utc"] = stamp()
                save_json(sidecar, record)
                self.counts["git_snapshots_deduplicated"] += 1
                self._retain_git_snapshots(slug, repo_dir)
                print(f"  [Dedup] Identico su tutti i ref Git: riuso {existing.name}", flush=True)
                return record

        short = main_sha[:12]
        kind = "main" if branch == "main" else "default"
        filename = (f"{repo['name']}__git-{kind}-{main_date}-{short}"
                    f"__state-{fingerprint[:20]}.tar.gz.age")
        archive = repo_dir / filename
        if archive.exists():
            # A corrupt previous archive must not be silently overwritten.
            filename = filename.replace(".tar.gz.age", f"__repair-{dt.datetime.now(dt.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')}.tar.gz.age")
            archive = repo_dir / filename
        try:
            with StreamingArchive(archive, self.args.recipient) as stream:
                stream.add_tree(folder / "git", f"multi-backup/repositories/{slug}/git")
                stream.add_json(f"multi-backup/repositories/{slug}/git_snapshot.json", {
                    "repository": slug, "main_branch_used": branch,
                    "main_commit_sha": main_sha, "main_commit_date_utc": main_date,
                    "all_refs_fingerprint": fingerprint,
                    "git_lfs_fetch_all_succeeded": True,
                    "git_fsck_succeeded": True,
                    "wiki_present_in_archive": bool(wiki and wiki.exists()),
                    "created_utc": stamp(),
                })
            checksum = self._checksum(archive)
            record = {
                "repository": slug, "filename": filename,
                "relative_archive": f"repositories/{slug}/{filename}",
                "created_utc": stamp(), "last_seen_utc": stamp(),
                "main_branch_used": branch, "main_commit_sha": main_sha,
                "main_commit_date_utc": main_date,
                "fingerprint": fingerprint, "archive_sha256": checksum,
                "archive_bytes": archive.stat().st_size,
            }
            save_json(archive.with_suffix(archive.suffix + ".json"), record)
            archive.with_suffix(archive.suffix + ".sha256").write_text(
                f"{checksum}  {filename}\n", encoding="utf-8")
            self.counts["git_snapshots_new"] += 1
            self._retain_git_snapshots(slug, repo_dir)
            print(f"  [Archivio Git] Creato {filename}", flush=True)
            return record
        except Exception as exc:
            self.problem(f"Creazione archivio Git {slug}", exc, critical=True)
            return None

    def backup_releases(self, slug, folder, archive=None):
        base = folder / "releases"
        releases = self.api(f"/repos/{slug}/releases?per_page=100",
                            base / "index.json", collection="")
        for r in releases or []:
            self.counts["releases"] += 1
            rid = str(r.get("id", "unknown"))
            if not rid.isdigit():
                self.problem(f"release ID {slug}", f"ID non valido: {rid}")
                continue
            dest = base / rid
            save_json(dest / "metadata.json", r)
            if not self.args.skip_release_assets and r.get("assets"):
                assets = dest / "assets"
                assets.mkdir(parents=True, exist_ok=True)
                self.gh(["release", "download", r["tag_name"], "--repo", slug,
                         "--dir", str(assets), "--clobber"],
                        f"Release assets {slug}:{r.get('tag_name')}", timeout=3600)
            if archive:
                archive.add_tree(dest, f"multi-backup/repositories/{slug}/releases/{rid}")
                shutil.rmtree(dest)
        if archive and (base / "index.json").exists():
            archive.add_tree(base / "index.json", f"multi-backup/repositories/{slug}/releases/index.json")
            shutil.rmtree(base)

    def backup_issues(self, slug, folder):
        base = f"/repos/{slug}"
        idir = folder / "issues"
        issues = self.api(f"{base}/issues?state=all&per_page=100",
                          idir / "all_including_prs.json", collection="") or []
        pulls = self.api(f"{base}/pulls?state=all&per_page=100",
                         idir / "pull_requests.json", collection="") or []
        self.api(f"{base}/comments?per_page=100", idir / "commit_comments.json", collection="")
        if self.args.skip_issue_details:
            return
        tasks = []
        for issue in issues:
            n = issue.get("number")
            if not isinstance(n, int): continue
            dst = idir / "threads" / str(n)
            if issue.get("comments", 0):
                tasks.append((f"{base}/issues/{n}/comments?per_page=100",dst/"comments.json",""))
            if not self.args.skip_issue_events:
                tasks.append((f"{base}/issues/{n}/events?per_page=100",dst/"events.json",""))
        for pr in pulls:
            n = pr.get("number")
            if not isinstance(n, int): continue
            dst = idir / "threads" / str(n)
            tasks.extend([
                (f"{base}/pulls/{n}/reviews?per_page=100",dst/"reviews.json",""),
                (f"{base}/pulls/{n}/comments?per_page=100",dst/"review_comments.json",""),
            ])
        if tasks:
            with ThreadPoolExecutor(max_workers=getattr(self.args, "metadata_workers", 4)) as executor:
                list(executor.map(lambda spec: self.api(spec[0], spec[1],collection=spec[2]),tasks))

    def backup_ci(self, slug, folder, archive=None):
        base = f"/repos/{slug}/actions"
        ci = folder / "ci"
        runs = self.api(f"{base}/runs?per_page=100",
                        ci / "runs_ALL_RETAINED.json", collection="workflow_runs")
        if runs is None:
            return
        limit = self.args.max_runs_per_repo
        # Following an applied cleanup, keep only runs still needed (default
        # branch, tags, active runs, or independently protected artifacts).
        # An unsuccessful inspection must never hide potentially useful CI data.
        report = self.ci_cleanup_reports.get(slug)
        if report and not report.get("dry_run") and report.get("keep_run_ids") is not None:
            allowed = set(report["keep_run_ids"])
            original_count = len(runs)
            runs = [r for r in runs if r.get("id") in allowed]
            save_json(ci / "runs_ALL_RETAINED.json", runs)
            print(f"  [CI] Run selezionate dopo cleanup: {len(runs)}/{original_count}", flush=True)
        selected = runs[:limit] if limit else runs
        if limit and len(runs) > limit:
            self.problem(f"{slug} CI", f"Esportati dettagli solo per {limit} run su {len(runs)}")
        for i, r in enumerate(selected, 1):
            rid = r.get("id")
            if not isinstance(rid, int):
                continue
            self.counts["ci_runs"] += 1
            rd = ci / "runs" / str(rid)
            save_json(rd / "run.json", r)
            if i % 25 == 1:
                print(f"  CI: {i}/{len(selected)}", flush=True)
            self.api(f"{base}/runs/{rid}/jobs?per_page=100",
                     rd / "jobs.json", collection="jobs")
            attempts = max(1, int(r.get("run_attempt") or 1))
            # The current attempt is already covered by jobs.json and logs.txt;
            # older rerun attempts require distinct endpoints and files.
            for attempt in range(1, attempts):
                self.api(f"{base}/runs/{rid}/attempts/{attempt}/jobs?per_page=100",
                         rd / "prior_attempts" / str(attempt) / "jobs.json", collection="jobs")
                if not self.args.skip_ci_logs:
                    atdst = rd / "prior_attempts" / str(attempt) / "logs.txt"
                    atdst.parent.mkdir(parents=True, exist_ok=True)
                    with atdst.open("wb") as fp:
                        if self.gh(["run", "view", str(rid), "--attempt", str(attempt),
                                    "--repo", slug, "--log"],
                                   f"CI log {slug}:{rid} attempt {attempt}",
                                   file=fp, timeout=1800) is None:
                            atdst.unlink(missing_ok=True)
            if not self.args.skip_ci_logs:
                rd.mkdir(parents=True, exist_ok=True)
                with (rd / "logs.txt").open("wb") as fp:
                    if self.gh(["run", "view", str(rid), "--repo", slug, "--log"],
                               f"CI log {slug}:{rid}", file=fp, timeout=1800) is not None:
                        self.counts["ci_log_downloads"] += 1
                    else:
                        (rd / "logs.txt").unlink(missing_ok=True)
            artifacts = self.api(f"{base}/runs/{rid}/artifacts?per_page=100",
                                 rd / "artifacts_metadata.json", collection="artifacts")
            # Guard against API/cache lag immediately following a successful
            # artifact DELETE: never re-archive its obsolete JSON record.
            if artifacts is not None and report and report.get("deleted_artifact_ids"):
                deleted_ids = set(report["deleted_artifact_ids"])
                artifacts = [a for a in artifacts if a.get("id") not in deleted_ids]
                save_json(rd / "artifacts_metadata.json", artifacts)
            if not self.args.skip_ci_artifacts and artifacts and any(
                    not a.get("expired", False) for a in artifacts):
                artdir = rd / "artifacts"
                artdir.mkdir(parents=True, exist_ok=True)
                if self.gh(["run", "download", str(rid), "--repo", slug, "--dir", str(artdir)],
                           f"CI artifacts {slug}:{rid}", timeout=3600) is not None:
                    self.counts["ci_artifact_runs"] += 1
            if archive:
                archive.add_tree(rd, f"multi-backup/repositories/{slug}/ci/runs/{rid}")
                shutil.rmtree(rd)
        if archive:
            index = ci / "runs_ALL_RETAINED.json"
            if index.exists():
                archive.add_tree(index, f"multi-backup/repositories/{slug}/ci/runs_ALL_RETAINED.json")
            shutil.rmtree(ci, ignore_errors=True)

    def backup_gists(self, archive=None):
        gists = self.api("/gists?per_page=100", self.root / "account" / "gists.json",
                         collection="")
        for gist in gists or []:
            ident = gist.get("id", "")
            if not re.fullmatch(r"[0-9a-f]+", ident):
                continue
            target = self.root / "gists" / (ident + ".git")
            target.parent.mkdir(parents=True, exist_ok=True)
            if self.executable(["git", "clone", "--mirror", "--quiet",
                                f"https://gist.github.com/{ident}.git", str(target)],
                               "Gist " + ident, timeout=600):
                self.counts["gists"] += 1
                if archive:
                    archive.add_tree(target, f"multi-backup/gists/{ident}.git")
                    shutil.rmtree(target)
        if archive:
            idx = self.root / "account" / "gists.json"
            if idx.exists():
                archive.add_tree(idx, "multi-backup/account/gists.json")
                idx.unlink()

    def backup_credentials(self, archive):
        if self.args.credentials_dir:
            source = self.args.credentials_dir.resolve()
            if not source.is_dir():
                raise ValueError("--credentials-dir deve indicare una directory esistente")
            # Never recursively ingest the live staging/output directory.
            for prohibited in (self.args.output_dir.resolve(),
                               (self.args.staging_dir or self.args.output_dir).resolve()):
                if (source == prohibited or source in prohibited.parents
                        or prohibited in source.parents):
                    raise ValueError("--credentials-dir non puo' contenere output-dir o staging-dir")
            for current, dirs, files in os.walk(source, followlinks=False):
                for child in dirs + files:
                    if (Path(current) / child).is_symlink():
                        raise ValueError("Symlink non consentito in --credentials-dir")
            archive.add_tree(source, "multi-backup/credentials/supplied_by_user")
            print("[Credentials] Inclusi direttamente nell'archivio cifrato, senza copie in staging", flush=True)
        if self.args.include_gh_token:
            raw = self.gh(["auth", "token"], "gh auth token", optional=False)
            if raw is None or not raw.strip():
                self.problem("gh auth token", "Token non disponibile", critical=True)
            else:
                archive.add_bytes("multi-backup/credentials/github_cli_oauth_token.txt", raw.strip() + b"\n")
                print("[Credentials] Incluso token GitHub CLI direttamente in archivio (ATTENZIONE: token attivo)", flush=True)
        archive.add_json("multi-backup/credentials/limitations.json", {
            "secret_values": "GitHub REST API non restituisce i valori dei secrets.",
            "not_exportable": ["password e passkeys GitHub", "codici 2FA non recuperabili",
                               "secrets non disponibili localmente", "private keys non possedute",
                               "credenziali di integrazioni esterne non disponibili"],
            "locally_supplied": bool(self.args.credentials_dir),
            "github_cli_token_included": bool(self.args.include_gh_token),
        })

    def finish(self, start):
        self.api("/rate_limit", self.root / "account" / "rate_limit_final.json")
        inventory = self.secret_inventory
        save_json(self.root / "secret_inventory_VALUES_NOT_RECOVERABLE_BY_API.json", inventory)
        save_json(self.root / "backup_manifest.json", {
            "tool_version": VERSION, "created_utc": stamp(), "started_utc": start,
            "archive_strategy": "encrypted streaming metadata archive + deduplicated per-repo git archives",
            "repository_git_snapshots": self.repository_snapshots,
            "git_retention_actions": self.retention_actions,
            "ci_cleanup": self.ci_cleanup_reports,
            "github_username": self.username, "counts": self.counts,
            "configuration": {"max_copies_per_repo": self.args.max_copies_per_repo,
                              "max_metadata_backups": self.args.max_metadata_backups,
                              "max_runs_per_repo": self.args.max_runs_per_repo,
                              "cleanup_ci_artifacts": self.args.cleanup_ci_artifacts,
                              "keep_ci_runs": self.args.keep_ci_runs,
                              "cleanup_ci_dry_run": self.args.cleanup_ci_dry_run,
                              "purge_obsolete_metadata": self.args.purge_obsolete_metadata,
                              "skip_ci_logs": self.args.skip_ci_logs,
                              "skip_ci_artifacts": self.args.skip_ci_artifacts,
                              "skip_release_assets": self.args.skip_release_assets,
                              "skip_issue_details": self.args.skip_issue_details,
                              "skip_issue_events": self.args.skip_issue_events,
                              "skip_org_settings": self.args.skip_org_settings},
            "secrets_found_metadata_only": len(inventory),
            "secrets_values_completeness": "NON VERIFICABILE: valori solo se forniti localmente",
            "critical_failures": sum(bool(e["critical"]) for e in self.errors),
            "warnings": sum(not e["critical"] for e in self.errors),
            "disaster_recovery_notice": "Metadati archiviati NON equivalgono a un ripristino automatico completo.",
        })
        save_json(self.root / "backup_errors.json", self.errors)
        (self.root / "HOW_TO_RESTORE.txt").write_text(
            "1. Verifica lo SHA256 dei file .age e conserva la chiave privata age.\n"
            "2. Decifra metadata: age -d ARCHIVIO_METADATA.age | tar -xz -C DESTINAZIONE\n"
            "3. Leggi multi-backup/backup_manifest.json e backup_errors.json.\n"
            "4. Nel manifest identifica repositories/OWNER/REPO/FILENAME.tar.gz.age.\n"
            "5. Decifra il singolo repo: age -d REPO.tar.gz.age | tar -xz -C DESTINAZIONE\n"
            "6. Crea un repo GitHub VUOTO, poi dal mirror .git: git push --mirror URL_NUOVA\n"
            "7. Git LFS: git lfs push --all URL_NUOVA; ripristina la wiki separatamente.\n"
            "8. --mirror sovrascrive/cancella ref remoti: usarlo solo su repo nuovo vuoto.\n"
            "9. Ripristina secrets, settings, runner e webhook dai JSON e dalle credenziali.\n"
            "10. Gli snapshot Git eliminati dalla retention non sono ripristinabili: usa\n"
            "    il manifest metadata piu' recente, che punta agli archivi Git correnti.\n",
            encoding="utf-8")



class StreamingArchive:
    """Write each item directly to the compressed+encrypted output stream.

    Files required by git/gh are staged only until that item is archived, then removed.
    The entire backup is never assembled uncompressed on disk.
    """
    def __init__(self, output_file: Path, recipient: str | None):
        self.output_file = output_file
        self.recipient = recipient
        self.process = None
        self.tar = None
        self.closed = False
        self.items = 0

    def __enter__(self):
        cmd = ["age", "-o", str(self.output_file)]
        cmd += ["-r", self.recipient] if self.recipient else ["-p"]
        self.process = subprocess.Popen(cmd, stdin=subprocess.PIPE,
                                        stderr=subprocess.PIPE)
        if not self.process.stdin:
            raise RuntimeError("Impossibile aprire lo stream di cifratura")
        try:
            # In Python 3.9-3.11, tarfile.open(mode="w|gz") does not accept
            # compresslevel. The default gzip level retains streaming support.
            self.tar = tarfile.open(fileobj=self.process.stdin, mode="w|gz")
        except Exception:
            self.process.kill()
            self.process.wait()
            self.process.stdin.close()
            self.process.stderr.close()
            self.output_file.unlink(missing_ok=True)
            raise
        return self

    @staticmethod
    def _filter(member):
        # Prevent accidentally exporting links, pipes, sockets or device nodes.
        if not (member.isdir() or member.isfile()):
            return None
        member.uid = member.gid = 0
        member.uname = member.gname = ""
        member.mode = 0o700 if member.isdir() else 0o600
        return member

    def add_tree(self, path: Path, name: str):
        if not path.exists():
            return
        if path.is_symlink():
            raise ValueError(f"Symlink non consentito nell'archivio: {path}")
        self.tar.add(str(path), arcname=name, recursive=True, filter=self._filter)
        self.items += 1

    def add_bytes(self, name: str, content: bytes):
        info = tarfile.TarInfo(name)
        info.size = len(content)
        info.mtime = int(time.time())
        info.mode = 0o600
        self.tar.addfile(info, io.BytesIO(content))
        self.items += 1

    def add_json(self, name: str, obj):
        self.add_bytes(name, (json.dumps(obj, indent=2, ensure_ascii=False) + "\n").encode())

    def __exit__(self, exc_type, exc, tb):
        if self.closed:
            return False
        self.closed = True
        if exc_type is not None:
            # Close the tar stream first (avoids destructor warnings), then abort.
            try:
                self.tar.close()
            except Exception:
                pass
            try:
                self.process.stdin.close()
            except Exception:
                pass
            if self.process.poll() is None:
                self.process.kill()
            self.process.wait()
            self.process.stderr.close()
            self.output_file.unlink(missing_ok=True)
            return False
        try:
            self.tar.close()
            self.process.stdin.close()
            error = self.process.stderr.read().decode("utf-8", "replace")
            rc = self.process.wait()
            self.process.stderr.close()
            if rc != 0:
                raise RuntimeError(f"Cifratura fallita (age rc={rc}): {error[:800]}")
            self.output_file.chmod(0o600)
        except Exception:
            if self.process.poll() is None:
                self.process.kill()
                self.process.wait()
            self.process.stderr.close()
            self.output_file.unlink(missing_ok=True)
            raise
        return False


def check_prerequisites(args):
    for exe in ["gh", "git", "age", "tar"]:
        if not shutil.which(exe):
            raise RuntimeError(f"Comando richiesto non trovato: {exe}")
    if not args.skip_gists and not shutil.which("git"):
        raise RuntimeError("Git non trovato")
    if run_cmd(["git", "lfs", "version"]).returncode != 0:
        raise RuntimeError("Git LFS manca. Installa git-lfs e riprova.")
    if run_cmd(["gh", "auth", "status"]).returncode != 0:
        raise RuntimeError("Login GitHub non valido. Esegui: gh auth login")
    if not args.recipient and not sys.stdin.isatty():
        raise RuntimeError("La password age richiede un terminale; usa --recipient per esecuzioni automatiche")


def build_parser():
    parser = argparse.ArgumentParser(description="Backup GitHub cifrato, con mirror, metadata, CI e credenziali opzionali")
    parser.add_argument("--output-dir", type=Path, default=Path.home() / "GitHub-Disaster-Backups")
    parser.add_argument("--staging-dir", type=Path, default=None,
                        help="Volume cifrato per singolo mirror/run temporaneo (default: output-dir)")
    parser.add_argument("--credentials-dir", type=Path, help="Directory locale con secrets e credenziali ORIGINALI da includere")
    parser.add_argument("--include-gh-token", action="store_true",
                        help="Include esplicitamente il token OAuth attivo usato da gh")
    parser.add_argument("--recipient", help="Chiave PUBBLICA age1... per backup automatici (alternativa a password age -p)")
    parser.add_argument("--max-copies-per-repo", "--max-repo-copies", type=int,
                        default=5, dest="max_copies_per_repo",
                        help="Massimo numero di snapshot Git DISTINTI per repository (default: 5). Identici sempre deduplicati")
    parser.add_argument("--max-metadata-backups", type=int, default=5,
                        help="Massimo numero di archivi globali metadata/CI, default: 5")
    parser.add_argument("--max-runs-per-repo", type=int, default=0,
                        help="0 = tutte le run ancora elencate da GitHub (default)")
    parser.add_argument("--cleanup-ci-artifacts", action="store_true",
                        help="Attiva pulizia Actions artifacts su GitHub PRIMA del backup (opt-in, irreversibile)")
    parser.add_argument("--keep-ci-runs", type=int, default=5,
                        help="CI completate da conservare sul branch predefinito (default 5), piu' 1 per tag")
    parser.add_argument("--cleanup-ci-dry-run", action="store_true",
                        help="Simula la pulizia senza eliminare artifact (richiede --cleanup-ci-artifacts)")
    parser.add_argument("--purge-obsolete-metadata", action="store_true",
                        help="Dopo backup completo senza errori, elimina i precedenti metadata cifrati (non gli snapshot Git)")
    parser.add_argument("--skip-ci-logs", action="store_true")
    parser.add_argument("--skip-ci-artifacts", action="store_true")
    parser.add_argument("--skip-release-assets", action="store_true")
    parser.add_argument("--skip-issue-details", action="store_true")
    parser.add_argument("--skip-issue-events", action="store_true")
    parser.add_argument("--skip-org-settings", action="store_true")
    parser.add_argument("--skip-gists", action="store_true")
    return parser


def main():
    os.umask(0o077)
    args = build_parser().parse_args()
    if args.keep_ci_runs < 1:
        raise ValueError("--keep-ci-runs deve essere >= 1")
    if args.cleanup_ci_dry_run and not args.cleanup_ci_artifacts:
        raise ValueError("--cleanup-ci-dry-run richiede --cleanup-ci-artifacts")
    if args.purge_obsolete_metadata and (not args.cleanup_ci_artifacts or args.cleanup_ci_dry_run):
        raise ValueError("--purge-obsolete-metadata richiede cleanup CI effettivo, non dry-run")
    if args.max_runs_per_repo < 0:
        raise ValueError("--max-runs-per-repo deve essere >= 0")
    if args.max_copies_per_repo < 1 or args.max_metadata_backups < 1:
        raise ValueError("--max-copies-per-repo e --max-metadata-backups devono essere >= 1")
    check_prerequisites(args)
    args.output_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
    target = args.staging_dir or args.output_dir
    target.mkdir(parents=True, exist_ok=True, mode=0o700)
    if target.stat().st_mode & 0o077:
        raise PermissionError("Directory temporanea accessibile ad altri utenti: chmod 700 o volume cifrato")
    # Separate archives: one immutable compressed encrypted Git snapshot per
    # repository state, plus one streamed encrypted metadata/CI archive per run.
    name = dt.datetime.now(dt.timezone.utc).strftime("github-disaster-metadata-%Y%m%dT%H%M%S%fZ.tar.gz.age")
    final = args.output_dir / "_metadata" / name
    final.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    if final.exists():
        raise FileExistsError(f"Il file esiste: {final}")
    start = stamp()
    print("Archivio Git separato per repository, compresso+cifrato in streaming.", flush=True)
    print("Snapshot IDENTICI deduplicati; max copie DISTINTE/repo:", args.max_copies_per_repo, flush=True)
    print("I metadata e la CI confluiscono in un secondo archivio cifrato in streaming.", flush=True)
    print("Staging temporaneo limitato al singolo mirror, release o run: " + str(target), flush=True)
    with tempfile.TemporaryDirectory(prefix=".multi-backup-item-", dir=target) as temp:
        root = Path(temp) / "multi-backup"
        root.mkdir(mode=0o700)
        backup = Backup(args, root)
        # Discovery and optional destructive preflight MUST precede opening
        # any backup archive or starting git/release/CI downloads.
        backup.backup_account()
        repositories = backup.get_repos()
        backup.counts["repositories"] = len(repositories)
        print(f"Repository accessibili: {len(repositories)}", flush=True)
        if args.cleanup_ci_artifacts:
            print(f"[CI cleanup] {'SIMULAZIONE' if args.cleanup_ci_dry_run else 'APPLICAZIONE'}; "
                  f"ultime {args.keep_ci_runs} CI/default branch + 1/tag; distribuzioni protette", flush=True)
            cleanup = GitHubCleanup(args.keep_ci_runs, dry_run=args.cleanup_ci_dry_run)
            for slug, repo in repositories.items():
                report = cleanup.run(repo)
                backup.ci_cleanup_reports[slug] = report
                n = report.get("counts", {})
                print(f"  [Cleanup] {slug}: candidati={n.get('eligible_artifacts', 0)} "
                      f"protetti={n.get('protected_artifacts', 0)} "
                      f"eliminati={len(report.get('deleted_artifact_ids', []))} "
                      f"errori={len(report.get('failed_artifact_ids', []))}", flush=True)
                if report.get("skipped_due_to_error") or report.get("failed_artifact_ids"):
                    backup.problem("GitHub Actions cleanup " + slug,
                                   report.get("error") or "Eliminazione parziale / bloccata per sicurezza",
                                   critical=True)
        with StreamingArchive(final, args.recipient) as archive:
            archive.add_tree(root / "account", "multi-backup/account")
            shutil.rmtree(root / "account")

            orgs = sorted({r["owner"]["login"] for r in repositories.values()
                           if r.get("owner", {}).get("type") == "Organization"})
            if not args.skip_org_settings:
                for org in orgs:
                    backup.backup_organization(org)
                    archive.add_tree(root / "organizations" / org,
                                     f"multi-backup/organizations/{org}")
                    shutil.rmtree(root / "organizations" / org)
            for i, (slug, repo) in enumerate(repositories.items(), 1):
                print(f"[{i}/{len(repositories)}] {slug}", flush=True)
                folder = root / "repositories" / slug
                repo_dir = args.output_dir / "repositories" / slug
                record = backup.backup_git_snapshot(slug, folder, repo, repo_dir)
                if record is not None:
                    backup.repository_snapshots[slug] = record
                shutil.rmtree(folder / "git", ignore_errors=True)

                backup.backup_settings(slug, folder, repo)
                archive.add_tree(folder / "settings", f"multi-backup/repositories/{slug}/settings")
                shutil.rmtree(folder / "settings", ignore_errors=True)

                backup.backup_releases(slug, folder, archive)
                backup.backup_issues(slug, folder)
                archive.add_tree(folder / "issues", f"multi-backup/repositories/{slug}/issues")
                shutil.rmtree(folder / "issues", ignore_errors=True)
                backup.backup_ci(slug, folder, archive)
                shutil.rmtree(folder, ignore_errors=True)
                print(f"  [Metadata] {i}/{len(repositories)}: scaricati e archiviati", flush=True)

            if not args.skip_gists:
                backup.backup_gists(archive)
            backup.backup_credentials(archive)
            backup.finish(start)
            for file in root.iterdir():
                if file.is_file():
                    archive.add_tree(file, "multi-backup/" + file.name)
            for extra in ("account", "credentials"):
                target_extra = root / extra
                if target_extra.exists():
                    archive.add_tree(target_extra, "multi-backup/" + extra)
            print(f"[Metadata] Scritti {archive.items} blocchi in streaming", flush=True)

        checksum = backup._checksum(final)
        final.with_suffix(final.suffix + ".sha256").write_text(
            f"{checksum}  {final.name}\n", encoding="utf-8")
        # Retain only the latest global metadata snapshots; never delete a Git
        # mirror due to metadata retention. Failure to finish doesn't prune.
        metadata = sorted(final.parent.glob("github-disaster-metadata-*.tar.gz.age"))
        # Encrypted old metadata cannot be edited in place. The explicit purge
        # discards old archives *only after* a complete new archive is written
        # and no GitHub, mirror, release, CI or encryption error was reported.
        clean_reports = all(not r.get("skipped_due_to_error") and not r.get("failed_artifact_ids")
                            for r in backup.ci_cleanup_reports.values())
        full_scope = not any((args.skip_ci_logs, args.skip_ci_artifacts, args.skip_release_assets,
                              args.skip_issue_details, args.skip_issue_events, args.skip_org_settings,
                              args.skip_gists, args.max_runs_per_repo))
        if args.purge_obsolete_metadata:
            if backup.errors or not clean_reports or not full_scope or not backup.repository_snapshots or len(backup.repository_snapshots) != len(repositories):
                print("[Retention] Purge metadata DISABILITATO: backup incompleto, avvisi/errori o opzioni --skip; "
                      "conservo gli archivi precedenti", flush=True)
            else:
                for previous in metadata:
                    if previous == final:
                        continue
                    previous.unlink()
                    previous.with_suffix(previous.suffix + ".sha256").unlink(missing_ok=True)
                    print(f"[Retention] Eliminato metadata obsoleto: {previous.name}", flush=True)
                metadata = [final]
        for previous in metadata[:-args.max_metadata_backups]:
            previous.unlink()
            previous.with_suffix(previous.suffix + ".sha256").unlink(missing_ok=True)
            print(f"[Retention] Rimosso metadata precedente: {previous.name}", flush=True)
        print(f"\nARCHIVIO METADATA: {final}\nDimensione: {final.stat().st_size:,} byte")
        print("SHA256:", checksum)
        print("Riepilogo:", json.dumps(backup.counts, indent=2, ensure_ascii=False))
        ncrit = sum(x["critical"] for x in backup.errors)
        print(f"Problemi critici: {ncrit}; avvisi: {len(backup.errors) - ncrit}")
        print("Leggi backup_manifest.json / backup_errors.json dentro l'archivio metadata.")
    print("Temporanei per elemento eliminati (cancellazione non forense).", flush=True)
    return 2 if ncrit else 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (KeyboardInterrupt, Exception) as e:
        import traceback
        traceback.print_exc()
        print(f"FALLITO: {e}", file=sys.stderr)
        sys.exit(1)
