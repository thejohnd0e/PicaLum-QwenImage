# Builds PicaLum QwenImage and the qil CLI from source.
#
#   .\build.ps1              app + CLI  ->  .\out\
#   .\build.ps1 -Installer   also builds the NSIS installer
#
# A binary you build yourself carries no "downloaded from the internet" mark,
# so Windows SmartScreen does not warn about it the way it does for an unsigned download.

param([switch]$Installer)
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

$workspaceVersionMatch = [regex]::Match((Get-Content -LiteralPath "Cargo.toml" -Raw), '(?m)^version\s*=\s*"([^"]+)"')
if (-not $workspaceVersionMatch.Success) { throw "Could not read the workspace package version from Cargo.toml." }
$workspaceVersion = $workspaceVersionMatch.Groups[1].Value
$tauriVersion = (Get-Content -LiteralPath "app\src-tauri\tauri.conf.json" -Raw | ConvertFrom-Json).version
if ($workspaceVersion -ne $tauriVersion) {
    throw "Cargo version $workspaceVersion does not match Tauri bundle version $tauriVersion; the app header and installer filename would disagree."
}

function Need($cmd, $why, $install) {
    if (-not (Get-Command $cmd -ErrorAction SilentlyContinue)) {
        Write-Host "Missing: $cmd ($why)" -ForegroundColor Red
        Write-Host "  Install it with:  $install" -ForegroundColor Yellow
        Write-Host "  Then open a new terminal and run .\build.ps1 again."
        exit 1
    }
}

Write-Host "Checking prerequisites..." -ForegroundColor Cyan
Need "cargo" "Rust toolchain" "winget install Rustlang.Rustup"
Need "npm" "Node.js, runs the Tauri CLI" "winget install OpenJS.NodeJS.LTS"

# Rust on Windows links with the MSVC C++ build tools.
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vc = if (Test-Path $vswhere) { & $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath } else { $null }
if (-not $vc) {
    Write-Host "Missing: Visual Studio C++ build tools (the Rust linker needs them)" -ForegroundColor Red
    Write-Host '  Install them with:  winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"' -ForegroundColor Yellow
    exit 1
}
Write-Host "  cargo $((cargo --version) -replace 'cargo ',''), node $(node --version), MSVC ok"
Write-Host "  version $workspaceVersion (app and installer)"

Write-Host "`nBuilding the qil CLI..." -ForegroundColor Cyan
cargo build --release -p qil-cli
if ($LASTEXITCODE) { exit $LASTEXITCODE }

Write-Host "`nBuilding the desktop app (first build takes a few minutes)..." -ForegroundColor Cyan
Push-Location app
npm ci --no-audit --no-fund
if ($LASTEXITCODE) { Pop-Location; exit $LASTEXITCODE }
if ($Installer) { npx tauri build } else { npx tauri build --no-bundle }
$code = $LASTEXITCODE
Pop-Location
if ($code) { exit $code }

New-Item -ItemType Directory -Force out | Out-Null
$appSource = "target\release\picalum-qwenimage.exe"
$appDestination = "out\picalum-qwenimage.exe"
try {
    Copy-Item -LiteralPath $appSource -Destination $appDestination -Force -ErrorAction Stop
} catch {
    $appDestination = "out\picalum-qwenimage_$workspaceVersion.exe"
    Copy-Item -LiteralPath $appSource -Destination $appDestination -Force
    Write-Host "  The standard app path is in use; saved the updated binary as $appDestination" -ForegroundColor Yellow
}
Copy-Item target\release\qil.exe out\ -Force
if ($Installer) {
    $installerName = "PicaLum QwenImage_${workspaceVersion}_x64-setup.exe"
    $installerPath = Join-Path "target\release\bundle\nsis" $installerName
    if (-not (Test-Path -LiteralPath $installerPath)) { throw "Expected versioned installer was not produced: $installerPath" }
    Copy-Item -LiteralPath $installerPath -Destination out\ -Force
}

Write-Host "`nDone." -ForegroundColor Green
Get-ChildItem out | ForEach-Object { "  {0,-45} {1,6:N1} MB" -f $_.Name, ($_.Length / 1MB) }
Write-Host "`nRun .\$appDestination. The first launch installs the engine and downloads the weights (~19.5 GB, once)."
