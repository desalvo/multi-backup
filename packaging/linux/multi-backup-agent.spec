Name:           multi-backup-agent
Version:        0.1.0
Release:        39.preview%{?dist}
Summary:        Native Rust authenticated backup agent (development preview)
License:        EUPL-1.2
URL:            https://github.com/desalvo/multi-backup
Source0:        multi-backup-agent
Source1:        multi-backup-agent.service
Source2:        LICENSE
Requires:       ca-certificates
Requires:       age

%description
Python-free Rust outbound authenticated agent for approved backup profiles.
The systemd service is disabled until agent enrollment and admin approval.

%prep
%build
%install
install -Dpm0755 %{SOURCE0} %{buildroot}%{_bindir}/multi-backup-agent
install -Dpm0644 %{SOURCE1} %{buildroot}%{_unitdir}/multi-backup-agent.service
install -Dpm0644 %{SOURCE2} %{buildroot}%{_licensedir}/multi-backup-agent/LICENSE

%files
%{_bindir}/multi-backup-agent
%{_unitdir}/multi-backup-agent.service
%license %{_licensedir}/multi-backup-agent/LICENSE
