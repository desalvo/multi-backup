#!/usr/bin/env bash
# Native Rust Debian 12 / 13 .deb packager. Builds only compiled Rust payloads.
set -euo pipefail
: "${DEBIAN_VERSION:?Set DEBIAN_VERSION=12 or 13}"
: "${DEBIAN_ARCH:?Set DEBIAN_ARCH=amd64 or arm64}"
case "$DEBIAN_VERSION:$DEBIAN_ARCH" in
  12:amd64|12:arm64|13:amd64|13:arm64) ;;
  *) echo 'Unsupported Debian target' >&2; exit 2 ;;
esac
source /etc/os-release
test "${VERSION_ID%%.*}" = "$DEBIAN_VERSION"
test "$(dpkg --print-architecture)" = "$DEBIAN_ARCH"
for src in LICENSE packaging/linux/multi-backup-rust.desktop assets/icon-256.png; do
  test -f "$src"
done
command -v cargo >/dev/null
cargo test --manifest-path rust/Cargo.toml --lib
cargo build --manifest-path rust/Cargo.toml --release --bin multi-backup-native
cargo build --manifest-path rust-desktop/Cargo.toml --release --bin multi-backup-desktop
rust-desktop/target/release/multi-backup-desktop --self-test
rust/target/release/multi-backup-native --version
version="0.1.0~preview1+deb$DEBIAN_VERSION"
stage="$(mktemp -d)"
unpack="$(mktemp -d)"
trap 'rm -rf "$stage" "$unpack"' EXIT
install -Dpm0755 rust-desktop/target/release/multi-backup-desktop "$stage/usr/bin/multi-backup-rust"
install -Dpm0755 rust/target/release/multi-backup-native "$stage/usr/bin/multi-backup-native"
install -Dpm0644 packaging/linux/multi-backup-rust.desktop "$stage/usr/share/applications/multi-backup-rust.desktop"
install -Dpm0644 assets/icon-256.png "$stage/usr/share/icons/hicolor/256x256/apps/multi-backup-rust.png"
install -Dpm0644 LICENSE "$stage/usr/share/doc/multi-backup-rust/copyright"
install -dm0755 "$stage/DEBIAN"
# Compute runtime dependencies from the target Debian system's shared libraries.
# dpkg-shlibdeps requires a source package's debian/control, even in direct
# package builds. Keep this metadata confined to the temporary build root.
mkdir -p "$stage/debian"
cat >"$stage/debian/control" <<'EOF'
Source: multi-backup-rust
Section: utils
Priority: optional
Maintainer: Multi Backup project <desalvo@users.noreply.github.com>
Standards-Version: 4.6.2

Package: multi-backup-rust
Architecture: any
Description: compiled Rust backup preview
 Native encrypted backup manager.
EOF
dependencies="$(cd "$stage" && dpkg-shlibdeps -O -e"$stage/usr/bin/multi-backup-rust" -e"$stage/usr/bin/multi-backup-native")"
rm -rf "$stage/debian"
case "$dependencies" in
  shlibs:Depends=*) libdeps="${dependencies#shlibs:Depends=}" ;;
  *) echo 'No dynamic library dependency inventory' >&2; exit 1 ;;
esac
cat >"$stage/DEBIAN/control" <<EOF
Package: multi-backup-rust
Version: $version
Section: utils
Priority: optional
Architecture: $DEBIAN_ARCH
Maintainer: Multi Backup project <desalvo@users.noreply.github.com>
Depends: age, git, git-lfs, gh, $libdeps
Homepage: https://github.com/desalvo/multi-backup
Description: Multi Backup encrypted backup manager (compiled Rust preview)
 Native Rust desktop and encrypted backup CLI. This experimental package
 includes no Python runtime, PyInstaller, or Tk dependencies.
EOF
mkdir -p packages
output="packages/multi-backup-rust_${version}_${DEBIAN_ARCH}.deb"
dpkg-deb --build --root-owner-group "$stage" "$output"
test "$(dpkg-deb --field "$output" Package)" = multi-backup-rust
test "$(dpkg-deb --field "$output" Architecture)" = "$DEBIAN_ARCH"
dpkg-deb --contents "$output" | grep -F '/usr/bin/multi-backup-rust'
dpkg-deb --contents "$output" | grep -F '/usr/bin/multi-backup-native'
dpkg-deb --contents "$output" | grep -F '/usr/share/applications/multi-backup-rust.desktop'
! dpkg-deb --contents "$output" | grep -Ei '(pyinstaller|python[0-9]|__pycache__|\.py$)'
dpkg-deb -x "$output" "$unpack"
cmp rust/target/release/multi-backup-native "$unpack/usr/bin/multi-backup-native"
cmp rust-desktop/target/release/multi-backup-desktop "$unpack/usr/bin/multi-backup-rust"
sha256sum "$output" >"$output.sha256"
echo "Verified Debian $DEBIAN_VERSION $DEBIAN_ARCH: $output"
