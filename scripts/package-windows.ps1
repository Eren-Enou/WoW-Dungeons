<#
.SYNOPSIS
    Packages Dungeon Sandbox for Windows as a self-contained folder and ZIP.

.DESCRIPTION
    Copies the release executables, the game content (assets/) and
    HOW_TO_PLAY.txt into dist/DungeonSandbox-<version>-windows-x64/, verifies
    the result, and zips it. Source code, build artifacts, logs, saves,
    screenshots and custom editor copies (assets/content/dungeons/90_*) are
    never included. Nothing is uploaded or published.

    Works in Windows PowerShell 5.1 and PowerShell 7.

.PARAMETER Build
    Run `cargo build --release` first (game and headless runner).

.PARAMETER OutDir
    Where to create the package (default: dist/ in the repository).

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1 -Build
#>
[CmdletBinding()]
param(
    [switch]$Build,
    [string]$OutDir
)

$ErrorActionPreference = 'Stop'

function Fail([string]$message) {
    Write-Host "ERROR: $message" -ForegroundColor Red
    exit 1
}

$repo = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path (Join-Path $repo 'Cargo.toml'))) {
    Fail "Cargo.toml not found in '$repo'. Keep this script in the repository's scripts\ folder."
}
if (-not $OutDir) { $OutDir = Join-Path $repo 'dist' }

# Version from [workspace.package] in Cargo.toml.
$version = $null
$inPackage = $false
foreach ($line in Get-Content (Join-Path $repo 'Cargo.toml')) {
    if ($line -match '^\s*\[(.+)\]\s*$') { $inPackage = ($Matches[1] -eq 'workspace.package') }
    elseif ($inPackage -and $line -match '^\s*version\s*=\s*"([^"]+)"') { $version = $Matches[1]; break }
}
if (-not $version) { Fail "Could not read the version from [workspace.package] in Cargo.toml." }

if ($Build) {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Fail "cargo not found on PATH. Install Rust from https://rustup.rs or build manually, then rerun without -Build."
    }
    Write-Host "Building release (this can take several minutes the first time)..."
    Push-Location $repo
    try {
        & cargo build --release -p dungeon_game -p dungeon_sim
        if ($LASTEXITCODE -ne 0) { Fail "cargo build --release failed (exit code $LASTEXITCODE). Fix the build errors above and rerun." }
    } finally {
        Pop-Location
    }
}

$release = Join-Path $repo 'target\release'
$gameExe = Join-Path $release 'dungeon-sandbox.exe'
$headlessExe = Join-Path $release 'dungeon-headless.exe'
if (-not (Test-Path $gameExe)) {
    Fail "Missing $gameExe. Rerun with -Build, or run 'cargo build --release' first."
}
$content = Join-Path $repo 'assets\content'
if (-not (Test-Path (Join-Path $content 'dungeons'))) {
    Fail "Missing $content\dungeons - the game content is required."
}
$howTo = Join-Path $repo 'packaging\HOW_TO_PLAY.txt'
if (-not (Test-Path $howTo)) { Fail "Missing $howTo." }

$name = "DungeonSandbox-$version-windows-x64"
$stage = Join-Path $OutDir $name
$zip = Join-Path $OutDir "$name.zip"

# Clean previous output for this version only.
New-Item -ItemType Directory -Force $OutDir | Out-Null
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
if (Test-Path $zip) { Remove-Item -Force $zip }
New-Item -ItemType Directory -Force $stage | Out-Null

Copy-Item $gameExe $stage
if (Test-Path $headlessExe) {
    Copy-Item $headlessExe $stage
} else {
    Write-Host "note: dungeon-headless.exe not built; packaging the game only." -ForegroundColor Yellow
}
Copy-Item $howTo $stage
foreach ($doc in 'README.md', 'CHANGELOG.md') {
    $p = Join-Path $repo $doc
    if (Test-Path $p) { Copy-Item $p $stage }
}

# Content: TOML data only, without custom editor copies.
$destContent = Join-Path $stage 'assets\content'
Get-ChildItem -Path $content -Recurse -File | Where-Object {
    $_.Extension -eq '.toml' -and $_.Name -notlike '90_*'
} | ForEach-Object {
    $rel = $_.FullName.Substring($content.Length).TrimStart('\')
    $dest = Join-Path $destContent $rel
    New-Item -ItemType Directory -Force (Split-Path -Parent $dest) | Out-Null
    Copy-Item $_.FullName $dest
}

# Models (glTF/GLB props) and their provenance record.
$models = Join-Path $repo 'assets\models'
if (Test-Path $models) {
    $destModels = Join-Path $stage 'assets\models'
    Get-ChildItem -Path $models -Recurse -File | Where-Object {
        $_.Extension -in @('.glb', '.gltf', '.bin', '.png', '.ktx2') -or $_.Name -eq 'PROVENANCE.md'
    } | ForEach-Object {
        $rel = $_.FullName.Substring($models.Length).TrimStart('\')
        $dest = Join-Path $destModels $rel
        New-Item -ItemType Directory -Force (Split-Path -Parent $dest) | Out-Null
        Copy-Item $_.FullName $dest
    }
}

# Verify the package before zipping.
$required = @('dungeon-sandbox.exe', 'HOW_TO_PLAY.txt', 'assets\content\dungeons')
foreach ($r in $required) {
    if (-not (Test-Path (Join-Path $stage $r))) { Fail "Package is missing $r." }
}
$dungeons = @(Get-ChildItem (Join-Path $stage 'assets\content\dungeons') -Filter *.toml)
if ($dungeons.Count -eq 0) { Fail "Package has no dungeon files in assets\content\dungeons." }
$forbidden = @(Get-ChildItem $stage -Recurse | Where-Object {
    $_.Name -in @('saves', 'logs', 'screenshots', 'target', 'src', 'Cargo.toml') -or
    $_.Extension -in @('.rs', '.pdb', '.log', '.json')
})
if ($forbidden.Count -gt 0) {
    Fail ("Package contains files that must not ship: " + (($forbidden | ForEach-Object { $_.FullName }) -join ', '))
}

Compress-Archive -Path $stage -DestinationPath $zip -CompressionLevel Optimal
if (-not (Test-Path $zip)) { Fail "ZIP was not created at $zip." }

$zipMb = [math]::Round((Get-Item $zip).Length / 1MB, 1)
$folderMb = [math]::Round(((Get-ChildItem $stage -Recurse -File | Measure-Object Length -Sum).Sum) / 1MB, 1)
Write-Host ""
Write-Host "Packaged Dungeon Sandbox $version" -ForegroundColor Green
Write-Host "  folder: $stage ($folderMb MB, $($dungeons.Count) dungeon files)"
Write-Host "  zip:    $zip ($zipMb MB)"
Write-Host "Test it: extract the ZIP somewhere else and double-click dungeon-sandbox.exe."
