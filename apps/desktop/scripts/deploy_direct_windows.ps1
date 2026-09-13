# deploy_direct_windows.ps1
# Direct-sale Windows build: rebuild sidecar -> NSIS installer -> archive ->
# hand off to Store Manager for review/publish. Mirrors deploy_direct_macos.sh's
# handoff step (see that script + this repo's CLAUDE.md "Every release
# updates the self-served store too"), but has no signing step: Windows
# ships unsigned per docs/development/release-readiness.md.
#
# Run from apps/desktop: powershell -File scripts\deploy_direct_windows.ps1

$ErrorActionPreference = "Stop"

# Store Manager's watched drop folder, reached over the same SMB share
# (\\172.16.1.1\Gaia) the macOS script mounts as /Volumes/Gaia. Using the UNC
# path directly instead of a drive letter so this doesn't depend on any one
# machine's mapped-drive setup.
$IngestRoot = "\\172.16.1.1\Gaia\04_DEV\store-manager\ingest"
$ProductSlug = "darkwave"
$ProductCategory = "Apps"
$DisplayName = "Darkwave"

$ProjectRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $ProjectRoot

# This is a Cargo workspace, so the build output lands under the workspace
# root's target/, not apps/desktop/src-tauri/target/ (see deploy_direct_macos.sh's
# own comment on this).
$WorkspaceRoot = Resolve-Path (Join-Path $ProjectRoot "..\..")

$Version = (node -p "require('./src-tauri/tauri.conf.json').version").Trim()

Write-Host "Darkwave v$Version - Direct Distribution (Windows)"
Write-Host "----------------------------------------------------------"

# Same version-ledger check the macOS scripts run -- this repo's version
# fields (tauri.conf.json/package.json/Cargo.toml) are shared across every
# machine once `git pull origin main` picks them up, so a stale Windows
# checkout building an already-approved version is the same mistake as a
# stale Mac one, just without Apple's 409 to catch it. No build-number
# check here: that's an App Store Connect-only constraint.
Write-Host "[0/4] Checking version against docs/macos/version-ledger.md..."
. (Join-Path $PSScriptRoot "check_version_ledger.ps1")
Test-VersionLedger -CurrentVersion $Version

# -- 1. Rebuild the sidecar --------------------------------------------------
# apps/desktop/src-tauri/binaries/ is gitignored (compiled, machine-specific),
# so a pull never brings a copy along -- rebuild every run, matching
# docs/development/windows-setup.md's "After every pull" sequence, or the
# build fails with "resource path ...similarity-worker-...exe doesn't exist".
Write-Host "[1/4] Rebuilding similarity-worker sidecar..."
cargo build -p similarity-worker --release --manifest-path (Join-Path $WorkspaceRoot "Cargo.toml")
if ($LASTEXITCODE -ne 0) { throw "similarity-worker build failed" }
$Triple = (rustc -vV | Select-String '^host:').Line.Split(' ')[1]
New-Item -ItemType Directory -Force -Path "src-tauri\binaries" | Out-Null
Copy-Item (Join-Path $WorkspaceRoot "target\release\similarity-worker.exe") `
  "src-tauri\binaries\similarity-worker-$Triple.exe" -Force

# -- 2. Build the installer ---------------------------------------------------
# direct-dist feature + tauri.direct.conf.json, same as macOS's build step.
# tauri.windows.conf.json is layered on top (Windows-only): it adds
# DirectML.dll to bundle.resources so the ONNX Runtime DirectML EP is
# present next to the installed .exe. Tauri's config merge is JSON Merge
# Patch (RFC 7396) — objects merge key by key, but arrays REPLACE the base
# array wholesale, they don't concatenate. bundle.resources is an array, so
# tauri.windows.conf.json must repeat every entry the base tauri.conf.json
# already lists there (models/*, for the Sonic Radar instrument-detection
# model) alongside DirectML.dll, or a Windows build silently drops the
# model bundling instead of augmenting it.
# --bundles nsis pinned explicitly (not relying on "targets":"all") so a
# stray MSI/WiX pass never runs alongside it.
Write-Host "[2/4] Building (direct-dist, NSIS)..."
npx tauri build --features direct-dist --config src-tauri\tauri.direct.conf.json --config src-tauri\tauri.windows.conf.json --bundles nsis
if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }

$InstallerName = "Darkwave_${Version}_x64-setup.exe"
$InstallerPath = Join-Path $WorkspaceRoot "target\release\bundle\nsis\$InstallerName"
if (-not (Test-Path $InstallerPath)) {
    throw "Expected installer not found at $InstallerPath (version mismatch with tauri.conf.json?)"
}

# -- 3. Archive locally --------------------------------------------------------
Write-Host "[3/4] Archiving locally..."
$BuildDest = "builds\direct_distribution\windows"
New-Item -ItemType Directory -Force -Path $BuildDest | Out-Null
Copy-Item $InstallerPath (Join-Path $BuildDest $InstallerName) -Force
Write-Host "  -> $BuildDest\$InstallerName (local archive)"

# -- 4. Hand off to Store Manager ----------------------------------------------
# Same shape as the macOS script's ingest drop: store-manager/ingest/<category>/
# <slug>/<version>/. If a macOS drop for this exact version already landed
# first, merge the "windows" key into its manifest.json instead of
# overwriting -- both platforms ship from one manifest per version (see
# docs/manifest-schema.md on the share). Otherwise write a fresh windows-only
# manifest.
Write-Host "[4/4] Staging for Store Manager review..."

if (Test-Path $IngestRoot) {
    $DropDir = Join-Path $IngestRoot "$ProductCategory\$ProductSlug\$Version"
    New-Item -ItemType Directory -Force -Path $DropDir | Out-Null

    # Fixed filename, not version-suffixed -- matches products.js's existing
    # darkwave entry (downloadFiles.windows: 'actual/Darkwave.exe').
    Copy-Item $InstallerPath (Join-Path $DropDir "Darkwave.exe") -Force

    $ManifestPath = Join-Path $DropDir "manifest.json"
    if (Test-Path $ManifestPath) {
        $Manifest = Get-Content $ManifestPath -Raw | ConvertFrom-Json
        $Manifest.files | Add-Member -MemberType NoteProperty -Name "windows" -Value "Darkwave.exe" -Force
    } else {
        $Manifest = [ordered]@{
            productSlug    = $ProductSlug
            displayName    = $DisplayName
            category       = $ProductCategory
            fulfillment    = "license"
            version        = $Version
            files          = [ordered]@{ windows = "Darkwave.exe" }
            notifyExisting = $false
        }
    }
    # Windows PowerShell 5.1's "utf8" encoding always writes a BOM, which the
    # watcher's JSON.parse() rejects outright ("Unexpected token '﻿'") --
    # every retry then fails silently (visible only via the ingest API's
    # /events log, not the dashboard Inbox). Write via .NET directly instead.
    $Json = $Manifest | ConvertTo-Json -Depth 10
    [System.IO.File]::WriteAllText($ManifestPath, $Json, (New-Object System.Text.UTF8Encoding $false))

    Write-Host "  -> $DropDir (Store Manager ingest -- pending_review once the watcher picks it up)"
    Write-Host ""
    Write-Host "OK: v$Version built and queued for review."
    Write-Host ""
    Write-Host "Next: open the Store Manager dashboard, Dry-run, review, then Publish."
    Write-Host "  http://192.168.178.146:5180"
} else {
    Write-Host "  WARNING: Store Manager ingest folder not reachable at $IngestRoot"
    Write-Host "    (NAS not mounted / off the LAN?). Installer is built but not yet queued --"
    Write-Host "    reconnect to the share and re-run this script, or drop it manually."
}
