param([switch]$SkipValidation, [switch]$PortableOnly, [string]$RuntimeDirectory)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repoRoot = Split-Path $PSScriptRoot -Parent
$stage = $null
Push-Location $repoRoot
try {
    if ($env:OS -ne 'Windows_NT' -or -not [Environment]::Is64BitOperatingSystem) {
        throw 'Build the native package on Windows x64.'
    }
    # Ship the licensed redistributable, never a copy from Windows/System32.
    if (-not $RuntimeDirectory) {
        $redistRoot = $env:VCToolsRedistDir
        if (-not $redistRoot) {
            $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
            if (-not (Test-Path -LiteralPath $vswhere)) { throw 'Visual Studio redistributables were not found. Set -RuntimeDirectory to the x64 CRT directory.' }
            $installation = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
            if (-not $installation) { throw 'No Visual C++ build tools installation was found.' }
            $redistRoot = Join-Path $installation 'VC/Redist/MSVC'
        }
        $runtime = Get-ChildItem -LiteralPath $redistRoot -Filter vcruntime140.dll -Recurse |
            Where-Object { $_.FullName -match '[\\/]x64[\\/]Microsoft\.VC[0-9]+\.CRT[\\/]vcruntime140\.dll$' -and $_.FullName -notmatch '[\\/]onecore[\\/]' } |
            Sort-Object { [version](($_.VersionInfo.FileVersion -split ' ')[0]) } -Descending | Select-Object -First 1
        if (-not $runtime) { throw 'The x64 Visual C++ redistributable runtime was not found.' }
        $RuntimeDirectory = $runtime.DirectoryName
    }
    $runtimePath = Join-Path $RuntimeDirectory 'vcruntime140.dll'
    if (-not (Test-Path -LiteralPath $runtimePath -PathType Leaf)) { throw 'RuntimeDirectory must contain the x64 redistributable vcruntime140.dll.' }
    $nsis = $null
    if (-not $PortableOnly) {
        $nsisCommand = Get-Command makensis -ErrorAction SilentlyContinue
        if ($nsisCommand) { $nsis = $nsisCommand.Source }
        else { $nsis = Join-Path ${env:ProgramFiles(x86)} 'NSIS/makensis.exe' }
        if (-not (Test-Path -LiteralPath $nsis -PathType Leaf)) {
            throw 'Install NSIS 3, or use -PortableOnly to build just the ZIP.'
        }
    }
    $metadata = & cargo metadata --format-version 1 --no-deps --locked --offline --manifest-path app/Cargo.toml
    if ($LASTEXITCODE -ne 0) { throw 'Could not read the native manifest.' }
    $nativeVersion = (($metadata | ConvertFrom-Json).packages | Where-Object name -eq 'orca-slint').version
    if ($nativeVersion -notmatch '^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$') { throw 'Invalid native version.' }
    if (-not $SkipValidation) {
        foreach ($project in @('crates/orca-core', 'crates/orca-services', 'app')) {
            $manifest = "$project/Cargo.toml"
            & cargo fmt --manifest-path $manifest --check
            if ($LASTEXITCODE -ne 0) { throw "Formatting failed: $project" }
            & cargo clippy --manifest-path $manifest --target-dir target/slint --jobs 2 --locked --offline --all-targets --no-deps -- -D warnings
            if ($LASTEXITCODE -ne 0) { throw "Clippy failed: $project" }
            & cargo test --manifest-path $manifest --target-dir target/slint --jobs 2 --locked --offline -- --test-threads=1
            if ($LASTEXITCODE -ne 0) { throw "Tests failed: $project" }
        }
    }
    & cargo build --release --locked --offline --features gpu --manifest-path app/Cargo.toml --target-dir target/slint --jobs 2
    if ($LASTEXITCODE -ne 0) { throw 'Native release compilation failed.' }
    $executable = Join-Path $repoRoot 'target/slint/release/orca-slint.exe'
    $actualVersion = (Get-Item -LiteralPath $executable).VersionInfo.ProductVersion
    if ($actualVersion -ne $nativeVersion) { throw "Executable version $actualVersion differs from $nativeVersion." }
    $releaseRoot = Join-Path $repoRoot "release/native-v$nativeVersion"
    $packageName = "Orca_${nativeVersion}_windows_x64_portable"
    $stage = Join-Path $repoRoot ("target/slint/package-" + [Guid]::NewGuid().ToString('N'))
    $package = Join-Path $stage $packageName
    New-Item -ItemType Directory -Path $package -Force | Out-Null
    New-Item -ItemType Directory -Path $releaseRoot -Force | Out-Null
    Copy-Item -LiteralPath $executable -Destination (Join-Path $package 'Orca.exe')
    Copy-Item -LiteralPath $runtimePath -Destination (Join-Path $package 'vcruntime140.dll')
    Copy-Item -LiteralPath (Join-Path $repoRoot 'LICENSE') -Destination $package
    @'
Extract the ZIP and open Orca.exe. Add music folders in Settings.
Song metadata and cover edits write your audio files. Collection name edits
update matching song tags; collection covers and playlists stay in Orca.
Back up %LOCALAPPDATA%\OrcaSlintTauri with Orca closed before upgrading.
Extract newer builds into a separate directory; your profile is reused.
Software rendering is the default; Orca.exe --gpu selects GPU rendering.
Keep the included Microsoft Visual C++ runtime DLL beside Orca.exe.
Release notes: https://github.com/shubham-pathak1/orca/releases
Report issues: https://github.com/shubham-pathak1/orca/issues
'@ | Set-Content -LiteralPath (Join-Path $package 'README.txt') -Encoding UTF8
    $commit = & git rev-parse HEAD
    $dirty = [bool](& git status --porcelain)
    $compiler = & rustc --version
    [ordered]@{
        version = $nativeVersion; platform = 'windows-x64'; frontend = 'slint';
        commit = "$commit"; uncommittedChanges = $dirty; compiler = "$compiler";
        builtUtc = [DateTime]::UtcNow.ToString('o'); rendererDefault = 'software'; features = @('gpu')
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $package 'BUILD-INFO.json') -Encoding UTF8
    $exeHash = (Get-FileHash -LiteralPath (Join-Path $package 'Orca.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
    $runtimeHash = (Get-FileHash -LiteralPath (Join-Path $package 'vcruntime140.dll') -Algorithm SHA256).Hash.ToLowerInvariant()
    @("$exeHash  Orca.exe", "$runtimeHash  vcruntime140.dll") | Set-Content -LiteralPath (Join-Path $package 'SHA256SUMS.txt') -Encoding ASCII
    $archive = Join-Path $releaseRoot "$packageName.zip"
    $temporaryArchive = Join-Path $stage "$packageName.zip"
    Compress-Archive -LiteralPath $package -DestinationPath $temporaryArchive -CompressionLevel Optimal
    Move-Item -LiteralPath $temporaryArchive -Destination $archive -Force
    $archiveHash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    $checksums = @("$archiveHash  $packageName.zip")
    if (-not $PortableOnly) {
        $icon = Get-ChildItem -LiteralPath (Join-Path $repoRoot 'target/slint/release/build') -Filter orca.ico -Recurse |
            Where-Object { $_.FullName -match 'orca-slint-[^\\]+[\\/]out[\\/]orca\.ico$' } |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if (-not $icon) { throw 'The native build did not produce an installer icon.' }
        $installer = Join-Path $releaseRoot "Orca_${nativeVersion}_windows_x64_setup.exe"
        $temporaryInstaller = Join-Path $stage 'Orca-setup.exe'
        $resourceVersion = ($nativeVersion -split '-')[0] + '.0'
        & $nsis /V3 /WX "/DVERSION=$nativeVersion" "/DVERSION_RESOURCE=$resourceVersion" "/DPACKAGE_DIR=$package" "/DOUTPUT_FILE=$temporaryInstaller" "/DICON_FILE=$($icon.FullName)" (Join-Path $PSScriptRoot 'windows-installer.nsi')
        if ($LASTEXITCODE -ne 0) { throw 'NSIS installer compilation failed.' }
        Move-Item -LiteralPath $temporaryInstaller -Destination $installer -Force
        $installerHash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
        $checksums += "$installerHash  $(Split-Path $installer -Leaf)"
        Write-Output "Ready: $installer"
    }
    $checksums | Set-Content -LiteralPath (Join-Path $releaseRoot 'SHA256SUMS.txt') -Encoding ASCII
    Write-Output "Ready: $archive"
    Write-Output "SHA256: $archiveHash"
    if ($dirty) { Write-Output 'BUILD-INFO records local uncommitted changes. Commit/tag the reviewed source before publishing.' }
} finally {
    if ($stage -and (Test-Path -LiteralPath $stage)) {
        $stagePath = (Resolve-Path -LiteralPath $stage).Path
        $targetRoot = (Resolve-Path -LiteralPath (Join-Path $repoRoot 'target/slint')).Path
        if (-not $stagePath.StartsWith($targetRoot + '\', [StringComparison]::OrdinalIgnoreCase) -or
            ((Get-Item -LiteralPath $stagePath).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw 'Refusing to remove a package staging directory outside the build output.'
        }
        Remove-Item -LiteralPath $stagePath -Recurse -Force
    }
    Pop-Location
}
