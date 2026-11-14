# Build independent Rust agent Windows x64 EXE installer. Never auto-start.
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
Set-Location (Split-Path -Parent $PSScriptRoot)
cargo test --manifest-path rust/Cargo.toml --lib agent_protocol
if ($LASTEXITCODE -ne 0) { throw 'Agent protocol test failed' }
cargo test --manifest-path rust/Cargo.toml --lib afs_volume
if ($LASTEXITCODE -ne 0) { throw 'AFS command validation failed' }
cargo build --manifest-path rust/Cargo.toml --release --bin multi-backup-agent
if ($LASTEXITCODE -ne 0) { throw 'Native agent compilation failed' }
$agent='rust/target/release/multi-backup-agent.exe'
if (!(Test-Path $agent)) { throw 'Missing agent exe' }
& $agent --version
if ($LASTEXITCODE -ne 0) { throw 'Agent version smoke failed' }
choco install age.portable -y --no-progress
if ($LASTEXITCODE -ne 0) { throw 'Missing age encryption runtime' }
$chocoDir=if ($env:ChocolateyInstall) { $env:ChocolateyInstall } else { 'C:\ProgramData\chocolatey' }
$ageHome=Join-Path $chocoDir 'lib/age.portable/tools'
$stage=Join-Path (Get-Location) 'dist-agent-windows'
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item $agent (Join-Path $stage 'multi-backup-agent.exe')
foreach ($name in @('age.exe','age-keygen.exe')) {
  $candidate=Get-ChildItem $ageHome -File -Filter $name -Recurse | Select-Object -First 1
  if (!$candidate) { throw "Missing $name" }
  Copy-Item $candidate.FullName (Join-Path $stage $name)
}
Copy-Item docs/AGENTS_AFS_R039_EN.md (Join-Path $stage 'AGENT-README.md')
New-Item -ItemType Directory -Force -Path installers | Out-Null
$inno='C:\Program Files (x86)\Inno Setup 6\ISCC.exe'
if (!(Test-Path $inno)) { throw 'Inno Setup not present' }
& $inno 'packaging/windows/rust-agent-preview.iss'
if ($LASTEXITCODE -ne 0) { throw 'Agent installer compilation failed' }
$installer='installers/multi-backup-agent-preview-windows-x64.exe'
if (!(Test-Path $installer)) { throw 'No agent installer created' }
Get-FileHash $installer -Algorithm SHA256 |
  ForEach-Object { "$($_.Hash.ToLowerInvariant())  $([System.IO.Path]::GetFileName($_.Path))" } |
  Set-Content -Encoding ascii installers/checksums-agent-windows-x64.txt
Write-Output 'Windows agent installer built; no agent registered or enabled.'
