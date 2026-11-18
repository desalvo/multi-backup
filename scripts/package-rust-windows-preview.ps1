# Standalone Rust Windows preview. No Python or PyInstaller files in output.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
cargo build --manifest-path rust/Cargo.toml --release
if ($LASTEXITCODE -ne 0) { throw 'Rust core build failed' }
cargo test --manifest-path rust-desktop/Cargo.toml --all-targets
if ($LASTEXITCODE -ne 0) { throw 'Native GUI unit tests failed' }
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
# Pin the independent Mesa LLVMpipe software OpenGL implementation. This is
# required by Windows Server GPU-less runners and users whose GPU driver is
# unavailable. Never download an unpinned/latest executable into the package.
$mesaVersion = '26.3.0'
$mesaUrl = "https://github.com/mmozeiko/build-mesa/releases/download/$mesaVersion/mesa-llvmpipe-x64-$mesaVersion.7z"
$mesaSha256 = '11dc079e356f33ce2ff84aa399c975da1a7bdd4c470f6eef4cf359ad8b9ca80f'
$mesaArchive = Join-Path $env:RUNNER_TEMP "mesa-llvmpipe-$mesaVersion.7z"
$mesaStage = Join-Path $env:RUNNER_TEMP "mesa-llvmpipe-unpacked-$mesaVersion"
if (Test-Path $mesaStage) { throw 'Mesa staging path already exists' }
Invoke-WebRequest -Uri $mesaUrl -OutFile $mesaArchive -ErrorAction Stop
$actualSha=(Get-FileHash -LiteralPath $mesaArchive -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualSha -ne $mesaSha256) { throw "Mesa software OpenGL SHA mismatch, refusing supply-chain payload: $actualSha" }
$sevenZip='C:\Program Files\7-Zip\7z.exe'
if (!(Test-Path $sevenZip)) { throw 'Trusted installed 7-Zip missing' }
New-Item -ItemType Directory -Force $mesaStage | Out-Null
& $sevenZip x -y $mesaArchive "-o$mesaStage" | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Mesa archive extraction failed' }
$glFiles=@(Get-ChildItem -LiteralPath $mesaStage -File -Filter 'opengl32.dll' -Recurse)
if ($glFiles.Count -ne 1) { throw "Expected exactly one pinned Mesa opengl32.dll, found $($glFiles.Count)" }
Copy-Item -LiteralPath $glFiles[0].FullName -Destination (Join-Path $bundle 'opengl32.dll')
Copy-Item -LiteralPath 'packaging/windows/mesa-llvmpipe-notice.txt' -Destination (Join-Path $bundle 'Mesa-LLVMpipe-NOTICE.txt')
$glSha=(Get-FileHash (Join-Path $bundle 'opengl32.dll') -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Output "Bundled pinned Mesa 26.3.0 software OpenGL x64, GL_DLL_SHA256=$glSha"

& (Join-Path $bundle 'MultiBackupRust.exe') --self-test
if ($LASTEXITCODE -ne 0) { throw 'Bundled Rust preview self-test failed' }
# Exercise the actual GUI's Mesa llvmpipe software rasterizer before packaging;
# a CLI-only or fake Win32 test is not accepted.
$previousMesa=$env:MULTI_BACKUP_FORCE_LLVMPIPE
try {
    $env:MULTI_BACKUP_FORCE_LLVMPIPE='1'
    $mesaGuiLog=Join-Path $env:RUNNER_TEMP 'mesa-llvmpipe-r217-native-gui.txt'
    & (Join-Path $bundle 'MultiBackupRust.exe') --smoke *>&1 |
        Tee-Object -FilePath $mesaGuiLog
    if ($LASTEXITCODE -ne 0) { throw 'Real Windows LLVMpipe egui renderer did not start' }
    if (!(Select-String -LiteralPath $mesaGuiLog -Pattern 'MULTI_BACKUP_GUI_RENDERER=Windows_Mesa_LLVMpipe' -Quiet)) {
        throw 'Actual Mesa GL renderer identity missing'
    }
    if (!(Select-String -LiteralPath $mesaGuiLog -Pattern 'RUST_GUI_PAGE_SMOKE=About' -Quiet)) {
        throw 'Real GUI did not traverse every page'
    }
} finally {
    $env:MULTI_BACKUP_FORCE_LLVMPIPE=$previousMesa
}
# Make the real packaged age.exe/age-keygen.exe available to the native Rust
# encrypted restore acceptance test. The installed GUI contains no Python.
$previousPath = $env:PATH
try {
    $env:PATH = "$(Join-Path $bundle 'bin');$previousPath"
    cargo test --manifest-path rust/Cargo.toml --test native_cli_restore_matrix
    if ($LASTEXITCODE -ne 0) { throw 'Native Windows encrypted restore acceptance failed' }
    cargo test --manifest-path rust/Cargo.toml --test plugin_dispatch_roundtrip
    if ($LASTEXITCODE -ne 0) { throw 'Native Windows plugin execution acceptance failed' }
    cargo test --manifest-path rust/Cargo.toml --test incremental_chain_roundtrip
    if ($LASTEXITCODE -ne 0) { throw 'Native Windows incremental chain acceptance failed' }
    cargo test --manifest-path rust/Cargo.toml --test garbage_collection_roundtrip
    if ($LASTEXITCODE -ne 0) { throw 'Native Windows garbage collection acceptance failed' }
} finally {
    $env:PATH = $previousPath
}
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
