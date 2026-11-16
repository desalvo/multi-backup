# Multi Backup — r051 CI cost policy (16 November 2026)

Release safety is unchanged; a skipped job is NOT a passing test.

Draft pull requests run only focused core/server/desktop validations and affected provider integration checks.
Full Production CI on a PR runs only when applying the full-ci label, changing from draft to Ready for review, or with workflow_dispatch selecting the target branch.
On main, source-code pushes and v* tags still invoke full production testing; documentation-only pushes do not.
MinIO is mandatory inside full CI. Its separate live workflow runs only when the PR label run-minio-ci is applied or on workflow_dispatch.
Ceph RGW uses only the run-ceph-ci PR label or workflow_dispatch. Current evidence is FAIL (three-MON/three-OSD quorum not reached); never report success from a skipped job.
Ceph retry upper bound reduced from 900 to 320 seconds, with diagnostic pods/cluster/events on failure.
RustFS auto-runs only for S3 engine/API changes and its workflow/script; GUI-only or documentation changes cannot trigger it.
Other live DB/Oracle/filesystem checks use narrow provider-specific paths, not all rust/**.
Pull-request fast checks cancel superseded attempts; explicit expensive Ceph and MinIO runs are not cancelled mid-flight.
After a PR source-code change, require a NEW full CI result on the exact final commit before promoting to RC or merging.
Use GitHub Actions rerun failed jobs if only a runner issue occurred with identical source, to avoid rebuilding successful checks.
Workflow dispatch on a newly added workflow may require merging its definition into the default branch; labels work while it remains in a draft PR.
No percentage cost reduction is claimed without actual GitHub billing/runtime measurements.

## r052 — important GitHub path-filter correction

On `pull_request`, GitHub evaluates path filters against the WHOLE PR diff; this made long PRs replay expensive live tests on unrelated commits even after r051. On `push` to `feature/**`, path filters evaluate ONLY FILES CHANGED BY THAT PUSH. All automatically triggered real provider checks and the fast Rust component checker now use `push` on feature branches (not duplicate `pull_request`). Changes restricted to workflow definitions run only the lightweight static policy check. Real Ceph/independent MinIO remain explicitly requested via PR labels or workflow_dispatch. Full Production CI on PR remains gated by `full-ci` label or ready_for_review; main source push and stable tags remain full. Fast Rust dorny/paths-filter compares `github.event.before` against `github.sha` for the exact push span; never rely on the cumulative PR diff. Correctness and release security are unchanged.
