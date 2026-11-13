#define AppName "Multi Backup"
#define AppVersion "0.1.0"
#define AppDisplayVersion "0.1.0-r015-rc1"
#define AppPublisher "Multi Backup contributors"
#define AppURL "https://github.com/desalvo/multi-backup"
[Setup]
; Resolve LICENSE, dist binaries and assets relative to repository root.
SourceDir=..\..
AppId={{A17C5D60-3E9C-4BCB-8131-F9BC68FDB2DD}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppDisplayVersion}
VersionInfoVersion=0.1.0.15
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
DefaultDirName={autopf}\Multi Backup
DefaultGroupName=Multi Backup
OutputDir=installers
OutputBaseFilename=multi-backup-windows11-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64
PrivilegesRequired=admin
UninstallDisplayIcon={app}\MultiBackupGUI.exe
SetupIconFile=assets\multi-backup.ico
LicenseFile=LICENSE
[Files]
Source: "dist\multi-backup.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "dist\MultiBackupGUI.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "dist\bin\*"; DestDir: "{app}\bin"; Flags: recursesubdirs createallsubdirs ignoreversion
[Tasks]
Name: desktopicon; Description: "Create a &desktop shortcut"; GroupDescription: "Additional icons:"; Flags: unchecked
[Icons]
Name: "{group}\Multi Backup"; Filename: "{app}\MultiBackupGUI.exe"
Name: "{autodesktop}\Multi Backup"; Filename: "{app}\MultiBackupGUI.exe"; Tasks: desktopicon
[Run]
Filename: "{app}\MultiBackupGUI.exe"; Description: "Launch Multi Backup"; Flags: nowait postinstall skipifsilent
