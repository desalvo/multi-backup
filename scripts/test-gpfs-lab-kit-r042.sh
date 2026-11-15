#!/usr/bin/env bash
# CI safe static qualification; never installs IBM packages or creates mounts.
set -euo pipefail
for s in scripts/gpfs-lab/*.sh scripts/test-real-cephfs-gpfs-ces-field-r042.sh; do
  bash -n "$s"
done
test -s docs/GPFS_DEVELOPER_LAB_R042_IT.md
grep -F '12 TB' docs/GPFS_DEVELOPER_LAB_R042_IT.md >/dev/null
grep -F 'runs-on: ubuntu-24.04' .github/workflows/cephfs-gpfs-real-field-r042.yml >/dev/null
grep -F 'runs-on: [self-hosted, gpfs-ci]' scripts/gpfs-lab/PRIVATE_WORKFLOW_TEMPLATE.yml >/dev/null
grep -F 'approved-public-main-sha' scripts/gpfs-lab/PRIVATE_WORKFLOW_TEMPLATE.yml >/dev/null
grep -F 'GitHub Free' scripts/gpfs-lab/PRIVATE_WORKFLOW_TEMPLATE.yml >/dev/null
if grep -q 'runs-on:.*self-hosted' .github/workflows/cephfs-gpfs-real-field-r042.yml; then echo 'FAIL public self-hosted job' >&2; exit 1; fi
grep -F 'workflow_dispatch:' .github/workflows/cephfs-gpfs-real-field-r042.yml >/dev/null
# Public PR validation is permitted ONLY for GitHub-hosted lint.
# Private physical lab access must not be selectable in any public workflow.
if grep -q 'runs-on:.*self-hosted' .github/workflows/cephfs-gpfs-real-field-r042.yml; then
  echo 'FAIL: public workflow exposes a physical lab runner' >&2; exit 1
fi
if grep -q 'curl.*ibm.*rpm' scripts/gpfs-lab/*.sh; then
  echo 'FAIL: vendor binary redistribution/downloader in public kit' >&2; exit 1
fi
if bash scripts/gpfs-lab/03-runner-preflight.sh /tmp /var/tmp ces-nfs >/dev/null 2>&1; then
  echo 'FAIL: ordinary folders accepted as real CES mounts' >&2; exit 1
fi
if bash scripts/gpfs-lab/03-runner-preflight.sh /tmp /var/tmp native >/dev/null 2>&1; then
  echo 'FAIL: unlicensed native GPFS profile accepted' >&2; exit 1
fi
echo 'PASS vendor-lab scripts syntax and fail-closed anti-fake mount policy (does not imply real GPFS certificate)'
