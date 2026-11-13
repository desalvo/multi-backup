Name:           multi-backup
Version:        0.1.0
Release:        3.rc1%{?dist}
Summary:        Multi-backend encrypted backup manager with native GUI
License:        EUPL-1.2
URL:            https://github.com/desalvo/multi-backup
BuildArch:      x86_64
Source0:        multi-backup
Source4:        multi-backup-gui
Source1:        LICENSE
Source2:        multi-backup.desktop
Source3:        multi-backup.png
Requires:       git
Requires:       git-lfs
Requires:       gh
Requires:       age
# GitHub engine requires gh, git-lfs and age. Native dependencies may differ by EL release.
# CI preflights external helper availability before publishing signed stable packages.
%description
Native GUI and CLI for resumable encrypted GitHub backups and a read-only,
incremental Dropbox backend. CI artifact cleanup is disabled by default.
%prep
%build
%install
install -Dpm 0755 %{SOURCE0} %{buildroot}%{_bindir}/multi-backup
install -Dpm 0755 %{SOURCE4} %{buildroot}%{_bindir}/multi-backup-gui
install -Dpm 0644 %{SOURCE2} %{buildroot}%{_datadir}/applications/multi-backup.desktop
install -Dpm 0644 %{SOURCE3} %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/multi-backup.png
install -Dpm 0644 %{SOURCE1} %{buildroot}%{_licensedir}/multi-backup/LICENSE
%files
%{_bindir}/multi-backup
%{_bindir}/multi-backup-gui
%{_datadir}/applications/multi-backup.desktop
%{_datadir}/icons/hicolor/256x256/apps/multi-backup.png
%license %{_licensedir}/multi-backup/LICENSE
