# Standalone Rust Windows preview. No Python or PyInstaller files in output.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
cargo build --manifest-path rust/Cargo.toml --release
if ($LASTEXITCODE -ne 0) { throw 'Rust core build failed' }
cargo build --manifest-path rust-desktop/Cargo.toml --release
if ($LASTEXITCODE -ne 0) { throw 'Rust desktop build failed' }
$gui = 'rust-desktop/target/release/multi-backup-desktop.exe'
$cli = 'rust/target/release/multi-backup-native.exe'
if (!(Test-Path $gui) -or !(Test-Path $cli)) { throw 'Missing Rust binaries' }
& $gui --self-test
if ($LASTEXITCODE -ne 0) { throw 'Native GUI self-test failed' }
$bundle = Join-Path $root 'dist-rust-win'
if (Test-Path $bundle) { Remove-Item -LiteralPath $bundle -Recurse -Force }
New-Item -ItemType Directory -Force -Path (Join-Path $bundle 'bin') | Out-Null
Copy-Item -LiteralPath $gui -Destination (Join-Path $bundle 'MultiBackupRust.exe')
Copy-Item -LiteralPath $cli -Destination (Join-Path $bundle 'bin/multi-backup-native.exe')
Copy-Item -LiteralPath 'LICENSE' -Destination $bundle
Copy-Item -LiteralPath 'README.IT.md' -Destination $bundle
Copy-Item -LiteralPath 'README.md' -Destination $bundle
choco install age.portable -y --no-progress
if ($LASTEXITCODE -ne 0) { throw 'Could not install age build-time dependency' }
$chocoRoot = if ($env:ChocolateyInstall) { $env:ChocolateyInstall } else { 'C:\ProgramData\chocolatey' }
$ageRoot = Join-Path $chocoRoot 'lib/age.portable/tools'
foreach ($name in @('age.exe','age-keygen.exe')) {
    $binary = Get-ChildItem -LiteralPath $ageRoot -Filter $name -File -Recurse | Select-Object -First 1
    if (!$binary) { throw "Could not locate actual $name" }
    Copy-Item -LiteralPath $binary.FullName -Destination (Join-Path $bundle 'bin')
}
foreach ($name in @('gh.exe','git-lfs.exe')) {
    $resolved = Get-Command $name -ErrorAction SilentlyContinue
    if (!$resolved -or !(Test-Path $resolved.Source)) { throw "Missing required native helper $name" }
    Copy-Item -LiteralPath $resolved.Source -Destination (Join-Path $bundle 'bin')
}
& (Join-Path $bundle 'bin/age.exe') --version
& (Join-Path $bundle 'bin/multi-backup-native.exe') --version
& (Join-Path $bundle 'MultiBackupRust.exe') --self-test
if ($LASTEXITCODE -ne 0) { throw 'Bundled Rust preview self-test failed' }
$zip = 'multi-backup-rust-preview-windows-x64.zip'
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $bundle '*') -DestinationPath $zip
$inno = 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe'
if (!(Test-Path $inno)) { throw 'Inno Setup 6 is required for the preview installer' }
& $inno 'packaging/windows/rust-native-preview.iss'
if ($LASTEXITCODE -ne 0) { throw 'Native Rust installer build failed' }
$installer = 'installers/multi-backup-rust-preview-windows-x64.exe'
if (!(Test-Path $installer)) { throw 'Native Rust preview installer missing' }
Get-FileHash $zip, $installer -Algorithm SHA256 |
    ForEach-Object { "$($_.Hash.ToLowerInvariant())  $([System.IO.Path]::GetFileName($_.Path))" } |
    Set-Content -Encoding ascii checksums-rust-windows-x64.txt
Write-Output 'Rust-only Windows GUI archive and installer verified.'
