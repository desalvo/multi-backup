#define MyAppName "Multi Backup Rust Preview"
#define MyAppVersion "0.1.0-r020"
#define MyAppPublisher "Alessandro De Salvo"
#define MyAppExeName "MultiBackupRust.exe"

[Setup]
AppId={{F15CDE49-FD13-4BD2-A5B7-8C9B1C36BD22}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL=https://github.com/desalvo/multi-backup
DefaultDirName={localappdata}\Programs\Multi Backup Rust Preview
DefaultGroupName={#MyAppName}
OutputDir=..\..\installers
OutputBaseFilename=multi-backup-rust-preview-windows-x64
Compression=lzma2
SolidCompression=yes
SetupIconFile=..\..\assets\multi-backup.ico
UninstallDisplayIcon={app}\MultiBackupRust.exe
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
DisableProgramGroupPage=yes
LicenseFile=..\..\LICENSE
VersionInfoVersion=0.1.0.20
VersionInfoDescription=Multi Backup Rust native preview
AppMutex=MultiBackupRustPreview

[Files]
Source: "..\..\dist-rust-win\MultiBackupRust.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\dist-rust-win\bin\*"; DestDir: "{app}\bin"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\..\dist-rust-win\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\dist-rust-win\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\dist-rust-win\README.IT.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Multi Backup Rust Preview"; Filename: "{app}\MultiBackupRust.exe"
Name: "{autodesktop}\Multi Backup Rust Preview"; Filename: "{app}\MultiBackupRust.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create desktop shortcut"; Flags: unchecked

[Run]
Filename: "{app}\MultiBackupRust.exe"; Description: "Launch Multi Backup Rust Preview"; Flags: nowait postinstall skipifsilent
