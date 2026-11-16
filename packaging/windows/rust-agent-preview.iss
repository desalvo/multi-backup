#define MyAppName "Multi Backup Agent Preview"
#define MyAppVersion "0.1.0-r039"

[Setup]
AppId={{7E07640E-5CA1-4C14-A185-EC702EE65CC7}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher=Multi Backup project
DefaultDirName={autopf}\Multi Backup Agent Preview
DefaultGroupName={#MyAppName}
OutputDir=..\..\installers
OutputBaseFilename=multi-backup-agent-preview-windows-x64
Compression=lzma2
SolidCompression=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
LicenseFile=..\..\LICENSE
VersionInfoVersion=0.1.0.39

[Files]
Source: "..\..\dist-agent-windows\multi-backup-agent.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\dist-agent-windows\age.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\dist-agent-windows\age-keygen.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\dist-agent-windows\AGENT-README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Multi Backup Agent"; Filename: "{cmd}"; Parameters: "/k ""{app}\multi-backup-agent.exe"" --version"

[Run]
; Never auto-start unregistered agents.
