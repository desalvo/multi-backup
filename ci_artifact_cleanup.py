"""Conservative, repository-agnostic GitHub Actions artifact retention.

Only Actions *artifacts* are deleted. Workflow runs, their logs, Git refs,
GitHub Releases, registry packages, GitHub Pages and distribution repositories
are never modified. When the provenance of an artifact is unclear, keep it.
"""
from __future__ import annotations

import base64
import json
import re
from urllib.parse import quote
import subprocess
from collections import defaultdict
from dataclasses import dataclass


class CleanupError(RuntimeError):
    pass


# These indicate potential build/distribution deliverables, NOT a repository list.
DISTRIBUTABLE = re.compile(
    r"(?:^|[^a-z0-9])(?:release|publish|dist|distribution|repository|repo|package|packages|"
    r"rpm|deb|apt|yum|dnf|apk|wheel|sdist|pypi|maven|npm|nuget|binary|binaries|"
    r"installer|installers|setup|sdk|archive|bundle|bundles|firmware|artifact|"
    r"container|image|images|docker|oci|helm|charts?|macos|osx|windows|linux|"
    r"signed|notarized|universal|amd64|arm64|x86_64|aarch64)(?:$|[^a-z0-9])",
    re.I,
)
# Test/diagnostic output is normally transient, unless a workflow consumes it.
TRANSIENT = re.compile(
    r"(?:^|[^a-z0-9])(?:test|tests|logs?|coverage|junit|pytest|report|reports|"
    r"lint|ruff|flake8|check|checks|debug|trace|benchmark|snapshot|sarif|"
    r"playwright|screenshots?)(?:$|[^a-z0-9])", re.I,
)
STRONG_DISTRIBUTABLE = re.compile(
    r"(?:^|[^a-z0-9])(?:release|publish|distribution|repository|packages?|rpm|deb|"
    r"apt|yum|dnf|sdk|installer|docker|oci|helm|binary|binaries|signed|"
    r"wheel|sdist|pypi|maven|npm|nuget)(?:$|[^a-z0-9])", re.I
)
EXTERNAL_CONSUMER = re.compile(
    r"(?is)(?:workflow_run\s*:.*?(?:download-artifact|gh\s+run\s+download)|"
    r"(?:download-artifact|gh\s+run\s+download).*?(?:run-id\s*:|workflow_run\.id|github\.event\.workflow_run)|"
    r"/actions/artifacts/|/actions/runs/.{0,80}/artifacts)", re.I
)
DEPLOY_PUBLISH = re.compile(
    r"(?i)(?:gh\s+release\s+(?:upload|create)|softprops/action-gh-release|"
    r"upload-release-asset|gh-pages|pages/deploy|docker/(?:build-push|login)-action|"
    r"(?:npm|twine|maturin|dotnet|cargo)\s+publish|"
    r"(?:apt|yum|dnf|rpm|reprepro|createrepo|helm|pypi).{0,40}(?:publish|upload|push|repo)|"
    r"(?:git|rsync|scp|aws|rclone|curl).{0,55}(?:push|upload|sync))"
)
SAFE_BRANCH_EVENTS = {"push", "workflow_dispatch", "schedule", "repository_dispatch", "workflow_call", "workflow_run"}


def _valid_id(value):
    return type(value) is int and value > 0


def _published_duplicate(artifact, releases, tag):
    """Require same release tag, exact asset name AND cryptographic SHA-256.

    GitHub Actions artifact ZIP and GitHub Release assets can share a filename
    while containing different data. Matching filenames alone is NOT proof.
    If an API doesn't expose both SHA-256 digests we fail closed.
    """
    aname = str(artifact.get("name") or "").casefold()
    digest = artifact.get("digest")
    if not aname or not tag or not isinstance(digest, str) or not re.fullmatch(r"sha256:[0-9a-fA-F]{64}", digest):
        return False
    alternatives = {aname, aname + ".zip"}
    return any(asset in alternatives and asset_digest == digest for release in releases
               if release.get("tag") == tag
               for asset, asset_digest in release.get("assets", {}).items())


def _run_tag(run, tags):
    ref = str(run.get("head_branch") or "")
    if ref in tags and run.get("event") in {"push", "create", "release", "workflow_dispatch"}:
        return ref
    if run.get("event") == "release":
        return ref if ref in tags else None
    return None


def select_artifacts(repo, runs, artifacts, tags, releases, workflows, keep_ci_runs):
    """Pure selection logic for fully fetched GitHub metadata.

    Returns a JSON-ready plan. Any unknown run/artifact is protected. `workflows`
    is a map workflow ID -> {'available':bool,'source':str,'path':str}.
    `releases` holds [{'tag': ..., 'assets': {lowercase names}, ...}].
    """
    if not isinstance(keep_ci_runs, int) or keep_ci_runs < 1:
        raise CleanupError("keep_ci_runs must be >= 1")
    branch = repo.get("default_branch")
    if not isinstance(branch, str) or not branch:
        raise CleanupError("Missing default branch")
    by_id = {}
    for run in runs:
        if not isinstance(run, dict) or not _valid_id(run.get("id")) or run["id"] in by_id:
            raise CleanupError("Invalid or duplicate workflow run ID")
        by_id[run["id"]] = run
    sorted_runs = sorted(runs, key=lambda r: (r.get("created_at") or "", r["id"]), reverse=True)
    keep_run_reasons = defaultdict(set)
    # For releases, prefer the newest successful run, then the newest run
    # containing an artifact, then the newest completed run as a last resort.
    has_artifacts = {a.get("workflow_run", {}).get("id") for a in artifacts
                     if isinstance(a, dict) and isinstance(a.get("workflow_run"), dict)}
    grouped_tags = defaultdict(list)
    for run in sorted_runs:
        tag = _run_tag(run, tags)
        if tag and run.get("status") == "completed":
            grouped_tags[tag].append(run)
    for tag, group in grouped_tags.items():
        preferred = (next((r for r in group if r.get("conclusion") == "success" and r["id"] in has_artifacts), None)
                     or next((r for r in group if r.get("conclusion") == "success"), None)
                     or next((r for r in group if r["id"] in has_artifacts), None)
                     or group[0])
        keep_run_reasons[preferred["id"]].add("best-completed-run-for-tag:" + tag)
    default_count = 0
    for run in sorted_runs:
        rid = run["id"]
        status = run.get("status")
        if status != "completed":
            keep_run_reasons[rid].add("run-active-or-unknown-status")
            continue
        tag = _run_tag(run, tags)
        if tag:
            # The selected tag run was chosen above; older tag attempts can
            # be removed unless needed by a distribution or other consumer.
            continue
        if run.get("event") == "release":
            keep_run_reasons[rid].add("release-event-unresolved-tag")
            continue
        if run.get("head_branch") == branch and run.get("event") in SAFE_BRANCH_EVENTS:
            if default_count < keep_ci_runs:
                default_count += 1
                keep_run_reasons[rid].add("latest-default-branch-run")
    external_consumers = any(w.get("available") and EXTERNAL_CONSUMER.search(w.get("source", ""))
                             for w in workflows.values())
    published = any(w.get("available") and DEPLOY_PUBLISH.search(w.get("source", ""))
                    for w in workflows.values())
    plan = []
    seen = set()
    for a in artifacts:
        if not isinstance(a, dict) or not _valid_id(a.get("id")) or a["id"] in seen:
            raise CleanupError("Invalid or duplicate artifact ID")
        seen.add(a["id"])
        wr = a.get("workflow_run") or {}
        rid = wr.get("id") if isinstance(wr, dict) else None
        run = by_id.get(rid)
        name = a.get("name")
        reasons = set(keep_run_reasons.get(rid, ()))
        if not run:
            reasons.add("run-missing-from-enumeration")
        if a.get("expired") is not False:
            reasons.add("expired-or-expiration-unknown")
        if not isinstance(name, str) or not name.strip():
            reasons.add("artifact-name-unknown")
        if run:
            wf = workflows.get(run.get("workflow_id"))
            if not wf or not wf.get("available"):
                reasons.add("workflow-source-unavailable")
            # Cross-run consumers may consume artifacts after their producer has
            # completed. There is no general REST API for outstanding consumers.
            if external_consumers:
                reasons.add("potential-cross-run-consumer")
            potential_package = bool(DISTRIBUTABLE.search(name or "")) and (
                not TRANSIENT.search(name or "") or bool(STRONG_DISTRIBUTABLE.search(name or "")))
            run_tag = _run_tag(run, tags)
            if potential_package:
                if not _published_duplicate(a, releases, run_tag):
                    reasons.add("potential-distribution-artifact-not-verifiably-published")
            elif published and wf and wf.get("available") and DEPLOY_PUBLISH.search(wf.get("source", "")):
                if not TRANSIENT.search(name or "") and not _published_duplicate(a, releases, run_tag):
                    reasons.add("artifact-of-publishing-workflow")
            if run.get("status") != "completed":
                reasons.add("run-not-completed")
        action = "keep" if reasons else "delete"
        plan.append({"id": a["id"], "run_id": rid, "name": name, "action": action,
                     "reasons": sorted(reasons) if reasons else ["outside-retention-window"]})
    kept_runs = {r["id"] for r in sorted_runs if keep_run_reasons.get(r["id"])}
    kept_runs.update(x["run_id"] for x in plan if x["action"] == "keep" and x["run_id"] in by_id)
    return {"repository": repo.get("full_name"), "default_branch": branch,
            "keep_ci_runs": keep_ci_runs, "tag_rule": "prefer latest successful artifact run; otherwise latest completed run per tag",
            "other_branches": "completed, unprotected artifacts eligible for deletion",
            "workflow_cross_run_consumers_detected": external_consumers,
            "distribution_publish_workflows_detected": published,
            "artifacts": plan,
            "keep_run_ids": sorted(kept_runs),
            "counts": {"total_artifacts": len(plan), "protected_artifacts": sum(x["action"] == "keep" for x in plan),
                       "eligible_artifacts": sum(x["action"] == "delete" for x in plan),
                       "runs_total": len(runs), "runs_retained_for_backup": len(kept_runs)}}


class GitHubCleanup:
    def __init__(self, keep_ci_runs=5, dry_run=False, runner=None, on_progress=None):
        self.keep_ci_runs = keep_ci_runs
        self.dry_run = dry_run
        self.runner = runner or subprocess.run
        self.on_progress = on_progress

    def _gh(self, args):
        try:
            res = self.runner(["gh", *args], stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                              check=False, timeout=240)
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise CleanupError(str(exc)) from exc
        if res.returncode != 0:
            raise CleanupError(res.stderr.decode("utf-8", "replace")[:900])
        return res.stdout

    def _api(self, endpoint):
        raw = self._gh(["api", endpoint])
        try:
            return json.loads(raw)
        except (TypeError, ValueError) as exc:
            raise CleanupError("Malformed JSON from " + endpoint) from exc

    def _collection(self, endpoint, key):
        raw = self._gh(["api", "--paginate", "--slurp", endpoint])
        try:
            pages = json.loads(raw)
            if not isinstance(pages, list):
                raise ValueError("Expected paginated list")
            result = []
            for page in pages:
                if key is None:
                    if not isinstance(page, list):
                        raise ValueError("Expected list")
                    result.extend(page)
                else:
                    if not isinstance(page, dict) or not isinstance(page.get(key), list):
                        raise ValueError("Missing list property " + key)
                    result.extend(page[key])
            return result
        except (TypeError, ValueError) as exc:
            raise CleanupError("Malformed paginated JSON from " + endpoint) from exc

    def inspect(self, repo):
        slug = repo.get("full_name")
        if not slug or not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", slug):
            raise CleanupError("Invalid repository name")
        if not isinstance(repo.get("default_branch"), str) or not repo.get("default_branch"):
            raise CleanupError("Default branch unavailable")
        base = "/repos/" + slug
        runs = self._collection(base + "/actions/runs?per_page=100", "workflow_runs")
        artifacts = self._collection(base + "/actions/artifacts?per_page=100", "artifacts")
        tags_list = self._collection(base + "/tags?per_page=100", None)
        tags = {t["name"] for t in tags_list if isinstance(t, dict) and isinstance(t.get("name"), str)}
        releases_list = self._collection(base + "/releases?per_page=100", None)
        releases = []
        for r in releases_list:
            if not isinstance(r, dict):
                raise CleanupError("Invalid release")
            tag = r.get("tag_name")
            if isinstance(tag, str):
                tags.add(tag)
            assets = r.get("assets")
            if not isinstance(assets, list):
                raise CleanupError("Incomplete release asset inventory")
            releases.append({"tag": tag, "assets": {a["name"].casefold(): a.get("digest")
                                                      for a in assets
                                                      if isinstance(a, dict) and isinstance(a.get("name"), str)
                                                      and (a.get("state") in (None, "uploaded"))
                                                      and not a.get("expired")}})
        # Sources of currently enabled workflows are inspected via API. Anything
        # unavailable is protected, not guessed away.
        wf_list = self._collection(base + "/actions/workflows?per_page=100", "workflows")
        workflows = {}
        for w in wf_list:
            if not isinstance(w, dict) or not _valid_id(w.get("id")):
                raise CleanupError("Invalid workflow list")
            path = w.get("path")
            content = ""
            available = False
            if isinstance(path, str) and path.startswith(".github/workflows/") and ".." not in path.split("/"):
                try:
                    doc = self._api(base + "/contents/" + "/".join(
                        quote(part, safe="") for part in path.split("/"))
                                    + "?ref=" + quote(repo["default_branch"], safe=""))
                    if isinstance(doc, dict) and doc.get("encoding") == "base64":
                        content = base64.b64decode(doc["content"], validate=False).decode("utf-8", "replace")
                        available = True
                except (CleanupError, ValueError, KeyError):
                    pass
            workflows[w["id"]] = {"available": available, "source": content, "path": path}
        report = select_artifacts(repo, runs, artifacts, tags, releases, workflows, self.keep_ci_runs)
        report["dry_run"] = self.dry_run
        report["deleted_artifact_ids"] = []
        report["failed_artifact_ids"] = []
        report["skipped_due_to_error"] = False
        return report

    def apply(self, repo, report):
        if self.dry_run:
            return report
        slug = repo["full_name"]
        candidates = [a for a in report["artifacts"] if a["action"] == "delete"]
        done = 0
        for a in report["artifacts"]:
            if a["action"] != "delete":
                continue
            rid, aid = a["run_id"], a["id"]
            try:
                # Revalidate before deletion to avoid races with new attempts.
                run = self._api(f"/repos/{slug}/actions/runs/{rid}")
                artifact = self._api(f"/repos/{slug}/actions/artifacts/{aid}")
                if run.get("status") != "completed" or artifact.get("workflow_run", {}).get("id") != rid:
                    raise CleanupError("Artifact provenance changed or run not completed")
                if artifact.get("name") != a["name"] or artifact.get("expired") is not False:
                    raise CleanupError("Artifact changed/expired after inspection")
                self._gh(["api", "--method", "DELETE", f"/repos/{slug}/actions/artifacts/{aid}"])
                report["deleted_artifact_ids"].append(aid)
                a["result"] = "deleted"
                done += 1
                if self.on_progress:
                    self.on_progress({"status": "deleted", "name": a["name"], "id": aid,
                                      "done": done, "total": len(candidates)})
            except (CleanupError, AttributeError, TypeError) as exc:
                report["failed_artifact_ids"].append(aid)
                report["keep_run_ids"].append(rid)
                a["result"] = "not-deleted"
                a["error"] = str(exc)[:700]
                # Fail closed after first error; no additional destructive calls.
                report["stopped_after_error"] = True
                if self.on_progress:
                    self.on_progress({"status": "failed", "name": a["name"], "id": aid,
                                      "done": done, "total": len(candidates)})
                # Other candidate artifacts were NOT deleted, hence cannot be
                # excluded from the current backup as though cleanup succeeded.
                report["keep_run_ids"] = None
                break
        if report["keep_run_ids"] is not None:
            report["keep_run_ids"] = sorted(set(report["keep_run_ids"]))
        return report

    def run(self, repo):
        try:
            report = self.inspect(repo)
            return self.apply(repo, report)
        except CleanupError as exc:
            return {"repository": repo.get("full_name"), "skipped_due_to_error": True,
                    "error": str(exc)[:1200], "deleted_artifact_ids": [],
                    "failed_artifact_ids": [], "dry_run": self.dry_run,
                    "keep_run_ids": None, "artifacts": [],
                    "counts": {"total_artifacts": 0, "eligible_artifacts": 0, "protected_artifacts": 0}}
