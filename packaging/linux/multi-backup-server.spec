Name:           multi-backup-server
Version:        0.1.0
Release:        64.preview%{?dist}
Summary:        Native Rust cluster-aware backup server
License:        EUPL-1.2
URL:            https://github.com/desalvo/multi-backup
Source0:        multi-backup-server
Source1:        multi-backup-server.service
Source2:        server.env.example
Source3:        multi-backup-snmp.service
Source4:        configure-baremetal-snmp.sh
Source5:        LICENSE
Source6:        BARE_METAL_CLUSTER_R060_IT.md
Requires:       ca-certificates
Requires:       age
Requires:       postgresql
Requires(pre):  shadow-utils
%description
Native Python-free Rust Multi Backup web/API server. Optional cluster
backed by Patroni PostgreSQL, encrypted S3 WORM storage and SNMP.
Server is disabled until administrator configures secure TLS secrets.
Preview only; no stable/production certification.
%prep
%build
%install
install -Dpm0755 %{SOURCE0} %{buildroot}%{_bindir}/multi-backup-server
install -Dpm0644 %{SOURCE1} %{buildroot}/usr/lib/systemd/system/multi-backup-server.service
install -Dpm0644 %{SOURCE3} %{buildroot}/usr/lib/systemd/system/multi-backup-snmp.service
install -Dpm0755 %{SOURCE4} %{buildroot}%{_bindir}/multi-backup-configure-snmp
install -Dm0600 %{SOURCE2} %{buildroot}%{_sysconfdir}/multi-backup/server.env
install -Dpm0644 %{SOURCE5} %{buildroot}%{_licensedir}/multi-backup-server/LICENSE
install -Dpm0644 %{SOURCE6} %{buildroot}%{_docdir}/multi-backup-server/BARE_METAL_CLUSTER_R060_IT.md
install -d -m0700 %{buildroot}%{_sharedstatedir}/multi-backup
install -d -m0700 %{buildroot}%{_localstatedir}/log/multi-backup
%pre
getent group multi-backup >/dev/null || groupadd -r multi-backup
getent passwd multi-backup >/dev/null || useradd -r -g multi-backup \
  -d /var/lib/multi-backup -s /sbin/nologin -M multi-backup
%post
systemctl daemon-reload >/dev/null 2>&1 || :
chown multi-backup:multi-backup /var/lib/multi-backup /var/log/multi-backup || :
%preun
if [ "$1" -eq 0 ];then
 systemctl --no-reload disable --now multi-backup-server.service >/dev/null 2>&1 || :
 systemctl --no-reload disable --now multi-backup-snmp.service >/dev/null 2>&1 || :
fi
%postun
systemctl daemon-reload >/dev/null 2>&1 || :
%files
%{_bindir}/multi-backup-server
%{_bindir}/multi-backup-configure-snmp
/usr/lib/systemd/system/multi-backup-server.service
/usr/lib/systemd/system/multi-backup-snmp.service
%dir %{_sysconfdir}/multi-backup
%config(noreplace) %attr(0600,root,root) %{_sysconfdir}/multi-backup/server.env
%attr(0700,multi-backup,multi-backup) %dir %{_sharedstatedir}/multi-backup
%attr(0700,multi-backup,multi-backup) %dir %{_localstatedir}/log/multi-backup
%doc %{_docdir}/multi-backup-server/BARE_METAL_CLUSTER_R060_IT.md
%license %{_licensedir}/multi-backup-server/LICENSE
