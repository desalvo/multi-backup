#!/usr/bin/env bash
# CREATE-ONLY fixture on pre-existing GPFS mount; never overwrite or delete.
set -euo pipefail
umask 077
TARGET="$1"
case "$TARGET" in /gpfs/*/mb-r042-*) ;; *) echo "FAIL: use new /gpfs/<filesystem>/mb-r042-<unique> directory" >&2; exit 1;; esac
test "$MB_GPFS_FIXTURE_APPROVAL" = CREATE-DISPOSABLE-R042-FIXTURE
test "$TARGET" != / && test ! -e "$TARGET" && test ! -L "$TARGET"
PARENT=$(dirname "$TARGET")
test -d "$PARENT" && test ! -L "$PARENT"
test "$(findmnt -rn -T "$PARENT" -o FSTYPE)" = gpfs
command -v mmgetstate >/dev/null
mkdir -- "$TARGET"
mkdir -- "$TARGET/subdir"
printf 'MB_EXPLICIT_TEST_FIXTURE_R042\n' > "$TARGET/MB_EXPLICIT_TEST_FIXTURE_R042"
printf 'Independent IBM GPFS CES client test\n' > "$TARGET/subdir/text.txt"
dd if=/dev/urandom of="$TARGET/test.bin" bs=1M count=16 status=none
sha256sum "$TARGET/test.bin" | awk '{print $1}' > "$TARGET/test.sha256"
chmod 0755 "$TARGET" "$TARGET/subdir"
chmod 0644 "$TARGET/test.bin" "$TARGET/test.sha256" "$TARGET/subdir/text.txt" "$TARGET/MB_EXPLICIT_TEST_FIXTURE_R042"
sync
echo "PASS new disposable data fixture: $TARGET. Snapshot MUST be created separately by authorized IBM admin."
