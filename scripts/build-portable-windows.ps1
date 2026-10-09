param()
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repoRoot = Split-Path $PSScriptRoot -Parent
$previousFlags = $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS
Push-Location $repoRoot
try {
    if ($env:OS -ne 'Windows_NT' -or -not [Environment]::Is64BitOperatingSystem) {
        throw 'Build the portable executable on Windows x64.'
    }
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $installation = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    $dumpbin = Get-ChildItem -LiteralPath (Join-Path $installation 'VC/Tools/MSVC') -Filter dumpbin.exe -Recurse |
        Where-Object { $_.FullName -match '[\\/]Hostx64[\\/]x64[\\/]dumpbin.exe$' } |
        Sort-Object FullName -Descending | Select-Object -First 1
    if (-not $dumpbin) { throw 'The x64 MSVC dependency inspector was not found.' }
    # An explicit target keeps static CRT flags away from host build scripts and procedural macros.
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = '-C target-feature=+crt-static'
    & cargo build --release --locked --offline --features gpu --target x86_64-pc-windows-msvc --manifest-path app/Cargo.toml --target-dir target/slint --jobs 2
    if ($LASTEXITCODE -ne 0) { throw 'Standalone portable compilation failed.' }
    $executable = Join-Path $repoRoot 'target/slint/x86_64-pc-windows-msvc/release/orca-slint.exe'
    $dependencies = & $dumpbin.FullName /DEPENDENTS $executable
    if ($LASTEXITCODE -ne 0) { throw 'Could not inspect portable dependencies.' }
    if ($dependencies -match '(?i)(vcruntime|msvcp|concrt|ucrtbase|api-ms-win-crt)[^\s]*\.dll') {
        throw 'The portable executable still imports a C/C++ runtime DLL.'
    }
    $version = (Get-Item -LiteralPath $executable).VersionInfo.ProductVersion
    if ($version -notmatch '^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$') { throw 'Invalid executable version.' }
    $releaseRoot = Join-Path $repoRoot "release/native-v$version"
    New-Item -ItemType Directory -Path $releaseRoot -Force | Out-Null
    $name = "Orca_${version}_windows_x64_portable.exe"
    Copy-Item -LiteralPath $executable -Destination (Join-Path $releaseRoot $name) -Force
    $checksumPath = Join-Path $releaseRoot 'SHA256SUMS.txt'
    $checksums = @()
    if (Test-Path -LiteralPath $checksumPath) {
        $checksums = @(Get-Content -LiteralPath $checksumPath | Where-Object { $_ -and -not $_.EndsWith("  $name") })
    }
    $hash = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
    $checksums += "$hash  $name"
    $checksums | Set-Content -LiteralPath $checksumPath -Encoding ASCII
    $dependencies | Write-Output
    Write-Output "Ready: $(Join-Path $releaseRoot $name)"
} finally {
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = $previousFlags
    Pop-Location
}
