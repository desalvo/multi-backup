#!/usr/bin/env bash
# Build and inspect native server RPM or DEB on the destination distro and CPU.
set -euo pipefail
cd "$(dirname "$0")/.."
: "${SERVER_PACKAGE_FORMAT:?Choose rpm or deb}"
binary="${MULTI_BACKUP_SERVER_BINARY:-rust-server/target/release/multi-backup-server}"
if [ ! -x "$binary" ]; then
  cargo build --manifest-path rust-server/Cargo.toml --release --bin multi-backup-server
fi
test -x "$binary"
mkdir -p packages
case "$SERVER_PACKAGE_FORMAT" in
  deb)
    arch="$(dpkg --print-architecture)"
    case "$arch" in amd64|arm64);;*) echo "Unsupported Debian architecture" >&2;exit 2;;esac
    stage="$(mktemp -d)"
    trap 'rm -rf -- "$stage"' EXIT
    install -Dpm0755 "$binary" "$stage/usr/bin/multi-backup-server"
    install -Dpm0755 scripts/configure-baremetal-snmp.sh "$stage/usr/bin/multi-backup-configure-snmp"
    install -Dpm0644 packaging/linux/multi-backup-server.service "$stage/usr/lib/systemd/system/multi-backup-server.service"
    install -Dpm0644 packaging/linux/multi-backup-snmp.service "$stage/usr/lib/systemd/system/multi-backup-snmp.service"
    install -Dm0600 packaging/linux/server.env.example "$stage/etc/multi-backup/server.env"
    install -Dpm0644 docs/BARE_METAL_CLUSTER_R060_IT.md "$stage/usr/share/doc/multi-backup-server/BARE_METAL_CLUSTER_R060_IT.md"
    install -Dpm0644 LICENSE "$stage/usr/share/doc/multi-backup-server/copyright"
    mkdir -p "$stage/var/lib/multi-backup" "$stage/var/log/multi-backup"
    chmod 0700 "$stage/var/lib/multi-backup" "$stage/var/log/multi-backup"
    install -d -m0755 "$stage/DEBIAN"
    cat >"$stage/DEBIAN/control" <<EOF
Package: multi-backup-server
Version: 0.1.0~preview64
Section: admin
Priority: optional
Architecture: $arch
Maintainer: Multi Backup <desalvo@users.noreply.github.com>
Depends: ca-certificates, age, postgresql-client, adduser
Homepage: https://github.com/desalvo/multi-backup
Description: Multi Backup native Rust cluster-aware server preview
 Service disabled by default. Administrator must configure Patroni/etcd.
EOF
    echo '/etc/multi-backup/server.env' >"$stage/DEBIAN/conffiles"
    cat >"$stage/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if ! getent group multi-backup >/dev/null;then addgroup --system multi-backup;fi
if ! getent passwd multi-backup >/dev/null;then
 adduser --system --ingroup multi-backup --home /var/lib/multi-backup \
   --no-create-home --disabled-login multi-backup
fi
chown multi-backup:multi-backup /var/lib/multi-backup /var/log/multi-backup
if command -v systemctl >/dev/null 2>&1;then systemctl daemon-reload || :;fi
exit 0
EOF
    chmod 0755 "$stage/DEBIAN/postinst"
    out="packages/multi-backup-server_0.1.0~preview64_$arch.deb"
    dpkg-deb --build --root-owner-group "$stage" "$out"
    test "$(dpkg-deb --field "$out" Package)" = multi-backup-server
    ;;
  rpm)
    case "$(uname -m)" in x86_64|aarch64);;*)echo "Unsupported RPM CPU" >&2;exit 2;;esac
    top="$HOME/rpmbuild"
    mkdir -p "$top/"{BUILD,RPMS,SOURCES,SPECS,SRPMS}
    install -m0755 "$binary" "$top/SOURCES/multi-backup-server"
    install -m0644 packaging/linux/multi-backup-server.service "$top/SOURCES/"
    install -m0644 packaging/linux/multi-backup-snmp.service "$top/SOURCES/"
    install -m0644 packaging/linux/server.env.example "$top/SOURCES/"
    install -m0755 scripts/configure-baremetal-snmp.sh "$top/SOURCES/"
    install -m0644 docs/BARE_METAL_CLUSTER_R060_IT.md "$top/SOURCES/"
    install -m0644 LICENSE "$top/SOURCES/"
    rpmbuild -bb packaging/linux/multi-backup-server.spec --define "_topdir $top"
    while IFS= read -r rpm;do
      cp "$rpm" packages/
      test "$(rpm -qp --queryformat '%{NAME}' "$rpm")" = multi-backup-server
    done < <(find "$top/RPMS" -type f -name 'multi-backup-server-*.rpm' | sort)
    out="$(find packages -type f -name 'multi-backup-server-*.rpm' -print -quit)"
    ;;
  *)echo "Unknown server package format" >&2;exit 2;;
esac
test -n "$out" && test -s "$out"
sha256sum "$out"
echo "Native server $SERVER_PACKAGE_FORMAT package created; not signed/certified"
