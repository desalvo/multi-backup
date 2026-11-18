#!/usr/bin/env bash
# Ubuntu24 real signed-source native Rust GUI/CLI/agent package and install test.
set -euo pipefail
cd "$(dirname "$0")/../.."
test "$RUNNER_ENVIRONMENT" = github-hosted
test "$(git rev-parse HEAD)" = "$GITHUB_SHA"
source /etc/os-release
test "$ID:$VERSION_ID:$(dpkg --print-architecture)" = ubuntu:24.04:amd64
test "$(id -u)" -ne 0
for p in rust/target/release/multi-backup-native rust/target/release/multi-backup-agent rust-desktop/target/release/multi-backup-desktop;do test -x "$p";done
command -v dpkg-shlibdeps
command -v age-keygen
tmp="$(mktemp -d)"
stage="$tmp/root"
package=multi-backup-rust-rc
version=0.1.0~rc0.r216+ubuntu24.04
cleanup(){ sudo dpkg --purge "$package" >/dev/null 2>&1 || true;rm -rf "$tmp"; }
trap cleanup EXIT
install -Dpm0755 rust-desktop/target/release/multi-backup-desktop "$stage/usr/bin/multi-backup-rust"
install -Dpm0755 rust/target/release/multi-backup-native "$stage/usr/bin/multi-backup-native"
install -Dpm0755 rust/target/release/multi-backup-agent "$stage/usr/bin/multi-backup-agent"
install -Dpm0644 assets/icon-256.png "$stage/usr/share/icons/hicolor/256x256/apps/multi-backup-rust.png"
install -Dpm0644 packaging/linux/multi-backup-rust.desktop "$stage/usr/share/applications/multi-backup-rust.desktop"
install -Dpm0644 LICENSE "$stage/usr/share/doc/$package/copyright"
mkdir -p "$stage/debian" "$stage/DEBIAN" packages
printf 'Source: %s\nSection: utils\nPriority: optional\nMaintainer: Multi Backup <desalvo@users.noreply.github.com>\nStandards-Version: 4.7.0\n\nPackage: %s\nArchitecture: any\nDescription: Compiled Rust RC\n Runtime preview.\n' "$package" "$package" > "$stage/debian/control"
shlibs="$(cd "$stage" && dpkg-shlibdeps -O -e"$stage/usr/bin/multi-backup-rust" -e"$stage/usr/bin/multi-backup-native" -e"$stage/usr/bin/multi-backup-agent")"
rm -rf "$stage/debian"
case "$shlibs" in shlibs:Depends=*) libdeps="$(printf '%s' "$shlibs" | cut -d= -f2-)" ;; *) exit 1;; esac
cat > "$stage/DEBIAN/control" <<EOF
Package: $package
Version: $version
Section: utils
Priority: optional
Architecture: amd64
Maintainer: Multi Backup <desalvo@users.noreply.github.com>
Depends: age, ca-certificates, git, git-lfs, gh, $libdeps
Homepage: https://github.com/desalvo/multi-backup
Description: Native Rust Multi Backup RC-preview (unsigned)
 Compiled native GUI, backup CLI and agent. No Python/PyInstaller payload.
 Requires separately reviewed provider scope and signature for production.
EOF
pkg="packages/$package-$version-amd64.deb"
dpkg-deb --build --root-owner-group "$stage" "$pkg"
test "$(dpkg-deb --field "$pkg" Package)" = "$package"
! dpkg-deb -c "$pkg" | grep -Ei '(pyinstaller|__pycache__|python[0-9]|\.py$)'
sudo apt-get install -y --no-install-recommends "./$pkg"
dpkg -s "$package" | grep -Fx 'Status: install ok installed'
/usr/bin/multi-backup-rust --self-test
/usr/bin/multi-backup-native --version
/usr/bin/multi-backup-agent --version
xvfb-run -a /usr/bin/multi-backup-rust --smoke > "$tmp/gui.log" 2>&1
grep -F 'RUST_GUI_PAGE_SMOKE=' "$tmp/gui.log"
mkdir -p "$tmp/src/sub"
printf 'r216 genuine installed GUI+CLI test\n' > "$tmp/src/sub/probe.bin"
age-keygen -o "$tmp/key" >/dev/null
chmod 0600 "$tmp/key"
recipient="$(sed -n 's/^# public key: //p' "$tmp/key" | head -n 1)"
test -n "$recipient"
cli=/usr/bin/multi-backup-native
"$cli" storage init --root "$tmp/store" --recipient "$recipient"
"$cli" plugins backup --input posix --output posix --source "$tmp/src" --root "$tmp/store" --identity "$tmp/key" --label "r216 installed" --namespace rc216 --json > "$tmp/back.json"
id="$(jq -er '.id | select(length == 64)' "$tmp/back.json")"
"$cli" storage verify --root "$tmp/store" --identity "$tmp/key" --id "$id"
"$cli" plugins restore --input posix --output posix --root "$tmp/store" --identity "$tmp/key" --id "$id" --destination "$tmp/restored"
cmp "$tmp/src/sub/probe.bin" "$tmp/restored/rc216/sub/probe.bin"
if "$cli" plugins restore --input posix --output posix --root "$tmp/store" --identity "$tmp/key" --id "$id" --destination "$tmp/restored" >/dev/null 2>&1;then
  echo 'Unsafe overwrite accepted' >&2;exit 1
fi
sudo apt-get purge -y "$package"
test ! -e /usr/bin/multi-backup-rust
test ! -e /usr/bin/multi-backup-native
test ! -e /usr/bin/multi-backup-agent
sha256sum "$pkg" > "$pkg.sha256"
printf '%s\n' "$GITHUB_SHA" > packages/SOURCE_SHA.txt
echo 'PASS actual Ubuntu native DEB install, GUI window smoke, age restore, collision denial and uninstall'
