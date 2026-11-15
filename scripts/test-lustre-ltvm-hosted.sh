#!/usr/bin/env bash
# A real Lustre kernel/filesystem QEMU smoke test inside a disposable
# GitHub-hosted runner. No host Lustre kernel modules, real storage or passwords.
set -euo pipefail
umask 077
[[ "$RUNNER_ENVIRONMENT" == "github-hosted" ]] || { echo "FAIL: refusing to install guest virtualization on a persistent/self-hosted runner" >&2; exit 1; }
test -c /dev/kvm || { echo "FAIL: no /dev/kvm on this GitHub-hosted machine (real Lustre QEMU test blocked)" >&2; exit 1; }
test -f tests/lustre/ltvm-guest-smoke.sh
bash -n tests/lustre/ltvm-guest-smoke.sh
WORK=$(mktemp -d)
REPO_ROOT=$(pwd)
cleanup() {
  if test -d "$WORK/lustre-test-vms"; then
    (cd "$WORK/lustre-test-vms" && sg ltvm -c './ltvm destroy co1-single --force --yes' >/dev/null 2>&1) || true
  fi
  rm -rf -- "$WORK"
}
trap cleanup EXIT
# The exact source version is pinned to avoid running moving upstream root
# setup scripts. ltvm itself pulls its documented published kernel/VM artifacts.
git clone --quiet https://github.com/lustre-tools/lustre-test-vms.git "$WORK/lustre-test-vms"
git -C "$WORK/lustre-test-vms" checkout --quiet --detach ed549196e2f2d97bb922de86c57c6424b77b0fe9
test "$(git -C "$WORK/lustre-test-vms" rev-parse HEAD)" = ed549196e2f2d97bb922de86c57c6424b77b0fe9
cd "$WORK/lustre-test-vms"
export LTVM_NO_UPDATE=1
# Allowed only because this is an ephemeral GitHub-managed VM with sudo.
sudo env LTVM_NO_UPDATE=1 ./ltvm install
sg ltvm -c 'LTVM_NO_UPDATE=1 ./ltvm doctor'
sg ltvm -c 'LTVM_NO_UPDATE=1 ./ltvm target fetch rocky9'
sg ltvm -c 'LTVM_NO_UPDATE=1 ./ltvm create co1-single --vcpus 2 --mem 4096 --mdt-disks 1 --ost-disks 3'
sg ltvm -c 'LTVM_NO_UPDATE=1 ./ltvm llmount co1-single'
# No unreviewed shell code from a network source is passed to the guest.
GUEST_B64=$(base64 -w0 "$REPO_ROOT/tests/lustre/ltvm-guest-smoke.sh")
sg ltvm -c "LTVM_NO_UPDATE=1 ./ltvm vm exec co1-single bash -lc 'printf %s $GUEST_B64 | base64 -d | bash'"
echo 'PASS real QEMU Lustre kernel, server, client, OST striping and RO mount smoke.'
echo 'NOT TESTED: coordinated filesystem snapshots or native Rust encrypted Lustre restore/metadata fidelity.'
