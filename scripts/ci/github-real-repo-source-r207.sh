#!/usr/bin/env bash
# Real GitHub API and genuine git --mirror/LFS; never creates remote repositories.
set -Eeuo pipefail
umask 077
test "$RUNNER_ENVIRONMENT" = github-hosted
test "$GITHUB_REPOSITORY" = desalvo/multi-backup
test -n "$GH_TOKEN"
CLI=rust/target/debug/multi-backup-native
test -x "$CLI"
for tool in git gh age age-keygen jq;do command -v "$tool" >/dev/null;done
W=$(mktemp -d /tmp/mb-github-source.XXXXXXXX)
trap 'rm -rf -- "$W"' EXIT
age-keygen -o "$W/agekey" >/dev/null 2>&1
RECIPIENT=$(age-keygen -y "$W/agekey")
gh api "/repos/$GITHUB_REPOSITORY" --jq .full_name > "$W/remote-id"
test "$(cat "$W/remote-id")" = "$GITHUB_REPOSITORY"
"$CLI" plugins github-repository-backup \
 --repository "$GITHUB_REPOSITORY" --output-dir "$W/private" \
 --recipient "$RECIPIENT" --identity "$W/agekey" --json > "$W/receipt.json"
jq -e '.provider=="github" and .verified and .encrypted and
 .remote_modified==false and .git_members>0 and .metadata_members>=2' "$W/receipt.json"
ARCHIVE=$(jq -r .git_archive "$W/receipt.json")
META=$(jq -r .metadata_archive "$W/receipt.json")
test -s "$ARCHIVE" && test -s "$META"
test -s "$ARCHIVE.sha256" && test -s "$META.sha256"
"$CLI" plugins github-repository-restore-offline \
 --repository "$GITHUB_REPOSITORY" --archive "$ARCHIVE" \
 --identity "$W/agekey" --destination "$W/restored-new" --json > "$W/restored.json"
jq -e '.provider=="github" and .full_git_fsck and
 .create_only_local_restore and .remote_repository_recreated==false and
 .restored_members>0' "$W/restored.json"
MIRROR="$W/restored-new/multi-backup/repositories/desalvo/multi-backup/git/multi-backup.git"
test -d "$MIRROR"
git -C "$MIRROR" fsck --full
git -C "$MIRROR" show-ref --verify refs/heads/main
# Decrypt metadata into a pipe; list only member NAMES to private scratch.
# grep -q directly in a pipe can issue SIGPIPE to upstream gzip/tar and
# spuriously fail an otherwise valid archive under pipefail.
age -d -i "$W/agekey" "$META" | gzip -dc | tar tf - > "$W/meta-member-list.txt"
grep -q 'repository.json' "$W/meta-member-list.txt"
grep -q 'references.txt' "$W/meta-member-list.txt"
echo 'PASS real GitHub remote mirror and separately encrypted metadata archive contents'
if "$CLI" plugins github-repository-restore-offline \
 --repository "$GITHUB_REPOSITORY" --archive "$ARCHIVE" \
 --identity "$W/agekey" --destination "$W/restored-new" >/dev/null 2>&1;then
 echo 'FAIL GitHub offline restore overwrote an existing directory' >&2;exit 1
fi
age-keygen -o "$W/wrong" >/dev/null 2>&1
if "$CLI" plugins github-repository-restore-offline \
 --repository "$GITHUB_REPOSITORY" --archive "$ARCHIVE" \
 --identity "$W/wrong" --destination "$W/unauthorized" >/dev/null 2>&1;then
 echo 'FAIL GitHub encrypted archive accepted unrelated age key' >&2;exit 1
fi
test ! -e "$W/unauthorized"
# A second identical snapshot must reuse its immutable encrypted hash ID.
"$CLI" plugins github-repository-backup \
 --repository "$GITHUB_REPOSITORY" --output-dir "$W/private" \
 --recipient "$RECIPIENT" --identity "$W/agekey" --json > "$W/second.json"
test "$(jq -r .fingerprint_sha256 "$W/receipt.json")" = \
     "$(jq -r .fingerprint_sha256 "$W/second.json")"
test "$(jq -r .git_archive "$W/receipt.json")" = \
     "$(jq -r .git_archive "$W/second.json")"
echo 'PASS actual GitHub remote API and git --mirror+LFS, encrypted full Git and metadata B-In, offline new mirror fsck, age/overwrite negative tests'
echo 'NOT CERTIFIED all-account settings, organization policy/secrets, full historical Actions artifacts or create-new remote GitHub repo'
