# check_version_ledger.ps1
# PowerShell mirror of check_version_ledger.sh (macOS) — dot-sourced by
# deploy_direct_windows.ps1. Reads the SAME docs/macos/version-ledger.md
# (it's one shared monorepo — see this repo's CLAUDE.md "Keep every dev
# machine on the same version"), because the version fields it enforces
# (tauri.conf.json/package.json/Cargo.toml) are the same files on every
# machine once `git pull origin main` picks them up. Windows has no App
# Store Connect build-number constraint, so this only checks the marketing
# version against the ledger's last Approved row, never a build number.
#
# Usage: . .\check_version_ledger.ps1 ; Test-VersionLedger -CurrentVersion $Version

function Get-VersionLedgerPath {
    # apps/desktop/scripts -> apps/desktop -> workspace root. $PSScriptRoot
    # inside a function resolves to the directory of the script the
    # function was *defined* in (this file), regardless of dot-sourcing --
    # unlike $MyInvocation.MyCommand.Path, which inside a function refers to
    # the function itself and has no file path.
    $projectRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
    $workspaceRoot = Resolve-Path (Join-Path $projectRoot "..\..")
    return Join-Path $workspaceRoot "docs\macos\version-ledger.md"
}

# Returns -1, 0, or 1 for $a <, ==, > $b (dotted numeric versions only).
function Compare-Semver {
    param([string]$a, [string]$b)
    $av = $a.Split('.') | ForEach-Object { [int]$_ }
    $bv = $b.Split('.') | ForEach-Object { [int]$_ }
    for ($i = 0; $i -lt 3; $i++) {
        $ai = if ($i -lt $av.Length) { $av[$i] } else { 0 }
        $bi = if ($i -lt $bv.Length) { $bv[$i] } else { 0 }
        if ($ai -lt $bi) { return -1 }
        if ($ai -gt $bi) { return 1 }
    }
    return 0
}

function Test-VersionLedger {
    param([Parameter(Mandatory = $true)][string]$CurrentVersion)

    $ledger = Get-VersionLedgerPath
    if (-not (Test-Path $ledger)) {
        Write-Warning "$ledger not found -- skipping version-ledger preflight check"
        return
    }

    $approvedVersion = $null
    foreach ($line in Get-Content $ledger) {
        if ($line -notmatch '^\|\s*(\d{4}-\d{2}-\d{2})\s*\|\s*([\d.]+)\s*\|\s*([\d—-]+)\s*\|\s*([^|]+?)\s*\|') {
            continue
        }
        $version = $Matches[2]
        $status = $Matches[4]
        if (-not $approvedVersion -and $status -like 'Approved*') {
            $approvedVersion = $version
            break
        }
    }

    if ($approvedVersion) {
        $cmp = Compare-Semver $CurrentVersion $approvedVersion
        if ($cmp -le 0) {
            Write-Host ""
            Write-Host "version-ledger check failed" -ForegroundColor Red
            Write-Host "  Current marketing version ($CurrentVersion) does not exceed the last"
            Write-Host "  APPROVED version recorded in docs\macos\version-ledger.md ($approvedVersion)."
            Write-Host "  Bump the version in tauri.conf.json, package.json, and Cargo.toml (all"
            Write-Host "  three -- see the ledger's `"must all move together`" note) before building --"
            Write-Host "  this keeps Windows on the same version as the macOS/MAS builds."
            Write-Host ""
            throw "version-ledger check failed: $CurrentVersion does not exceed approved version $approvedVersion"
        }
    }

    Write-Host "version-ledger check passed -- building $CurrentVersion (last approved: $(if ($approvedVersion) { $approvedVersion } else { 'none' }))"
}
