Name:           multi-backup-rust
Version:        0.1.0
Release:        20.preview%{?dist}
Summary:        Native Rust encrypted multi-provider backup desktop (preview)
License:        EUPL-1.2
URL:            https://github.com/desalvo/multi-backup
BuildArch:      x86_64
Source0:        MultiBackupRust
Source1:        multi-backup-native
Source2:        LICENSE
Source3:        multi-backup-rust.desktop
Source4:        multi-backup.png
Requires:       git
Requires:       git-lfs
Requires:       gh
Requires:       age

%description
Fully compiled Rust preview for encrypted GitHub/Dropbox backup, authenticated
archive restore, protected vault and native graphical interface. Python,
Tk and PyInstaller are not included. Requires the documented native helper
packages. This package is an early preview, not a signed stable build.

%prep
%build
%install
install -Dpm 0755 %{SOURCE0} %{buildroot}%{_bindir}/multi-backup-rust
install -Dpm 0755 %{SOURCE1} %{buildroot}%{_bindir}/multi-backup-native
install -Dpm 0644 %{SOURCE2} %{buildroot}%{_licensedir}/multi-backup-rust/LICENSE
install -Dpm 0644 %{SOURCE3} %{buildroot}%{_datadir}/applications/multi-backup-rust.desktop
install -Dpm 0644 %{SOURCE4} %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/multi-backup-rust.png

%files
%{_bindir}/multi-backup-rust
%{_bindir}/multi-backup-native
%{_datadir}/applications/multi-backup-rust.desktop
%{_datadir}/icons/hicolor/256x256/apps/multi-backup-rust.png
%license %{_licensedir}/multi-backup-rust/LICENSE
