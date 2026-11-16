#!/usr/bin/env bash
# Build native agent installer on its real host distro/arch, without Python.
set -euo pipefail
cd "$(dirname "$0")/.."
test -n "$AGENT_FORMAT"
cargo test --manifest-path rust/Cargo.toml --lib agent_protocol
cargo test --manifest-path rust/Cargo.toml --lib afs_volume
cargo build --manifest-path rust/Cargo.toml --release --bin multi-backup-agent
binary="rust/target/release/multi-backup-agent"
test -x "$binary"
# Match the semantic package version and a real Rust native-preview
# revision, not a frozen historical r039 marker (the agent now reports r057).
"$binary" --version | grep -Eq '^multi-backup-agent 0[.]1[.]0-r[0-9]+-native-preview [(]build '
mkdir -p packages
case "$AGENT_FORMAT" in
  pkg)
    arch="$(uname -m)"
    case "$arch" in arm64|x86_64);; *) echo "Unsupported macOS agent arch" >&2; exit 2;; esac
    stage="$(mktemp -d)"
    trap 'rm -rf "$stage"' EXIT
    mkdir -p "$stage/usr/local/bin" "$stage/usr/local/share/multi-backup-agent"
    install -m0755 "$binary" "$stage/usr/local/bin/multi-backup-agent"
    install -m0644 LICENSE "$stage/usr/local/share/multi-backup-agent/LICENSE"
    install -m0644 docs/AGENTS_AFS_R039_IT.md "$stage/usr/local/share/multi-backup-agent/AGENT-README-IT.md"
    install -m0644 docs/AGENTS_AFS_R039_EN.md "$stage/usr/local/share/multi-backup-agent/AGENT-README-EN.md"
    install -m0644 packaging/macos/multi-backup-agent.plist.example "$stage/usr/local/share/multi-backup-agent/launchagent.plist.example"
    out="packages/multi-backup-agent-preview-macos-$arch.pkg"
    pkgbuild --root "$stage" --identifier eu.adsnet.multi-backup-agent.preview --version 0.1.0.39 --install-location / "$out"
    test -s "$out"
    ;;
  deb)
    test "$(dpkg --print-architecture)" = "$DEBIAN_ARCH"
    source /etc/os-release
    test "$(echo "$VERSION_ID" | cut -d. -f1)" = "$DEBIAN_VERSION"
    stage="$(mktemp -d)"
    trap 'rm -rf "$stage"' EXIT
    install -Dpm0755 "$binary" "$stage/usr/bin/multi-backup-agent"
    install -Dpm0644 packaging/linux/multi-backup-agent.service "$stage/usr/lib/systemd/system/multi-backup-agent.service"
    install -Dpm0644 docs/AGENTS_AFS_R039_IT.md "$stage/usr/share/doc/multi-backup-agent/README.IT.md"
    install -Dpm0644 LICENSE "$stage/usr/share/doc/multi-backup-agent/copyright"
    install -dm0755 "$stage/DEBIAN"
    cat > "$stage/DEBIAN/control" <<EOF
Package: multi-backup-agent
Version: 0.1.0~preview39+deb$DEBIAN_VERSION
Section: admin
Priority: optional
Architecture: $DEBIAN_ARCH
Maintainer: Multi Backup project <desalvo@users.noreply.github.com>
Depends: ca-certificates, age
Homepage: https://github.com/desalvo/multi-backup
Description: Multi Backup native Rust agent development preview
 Agent disabled by default until TLS enrollment and admin approval.
EOF
    out="packages/multi-backup-agent_0.1.0~preview39+deb$DEBIAN_VERSION"_"$DEBIAN_ARCH.deb"
    dpkg-deb --build --root-owner-group "$stage" "$out"
    test "$(dpkg-deb --field "$out" Package)" = multi-backup-agent
    ! dpkg-deb --contents "$out" | grep -Ei 'python|pyinstaller|tkinter'
    ;;
  rpm)
    case "$AGENT_DISTRO:$AGENT_EL" in
      rocky:8|rocky:9|rocky:10|alma:8|alma:9|alma:10);;
      *) echo "Unsupported EL agent OS" >&2;exit 2;;
    esac
    top="$HOME/rpmbuild"
    mkdir -p "$top/BUILD" "$top/RPMS" "$top/SOURCES" "$top/SPECS" "$top/SRPMS"
    install -m0755 "$binary" "$top/SOURCES/multi-backup-agent"
    install -m0644 packaging/linux/multi-backup-agent.service "$top/SOURCES/"
    install -m0644 LICENSE "$top/SOURCES/"
    rpmbuild -bb packaging/linux/multi-backup-agent.spec --define "_topdir $top"
    while IFS= read -r rpm; do
      out="packages/multi-backup-agent-$AGENT_DISTRO-el$AGENT_EL-$(basename "$rpm")"
      cp "$rpm" "$out"
      test "$(rpm -qp --queryformat '%{NAME}' "$out")" = multi-backup-agent
    done < <(find "$top/RPMS" -type f -name 'multi-backup-agent-*.rpm' | sort)
    ;;
  *) echo "Unknown agent package format" >&2;exit 2;;
esac
find packages -maxdepth 1 -name '*multi-backup-agent*' -type f -print | grep -q .
echo "Native agent $AGENT_FORMAT installer created (service not enabled)."
