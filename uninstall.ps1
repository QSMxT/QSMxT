# QSMxT.rs uninstaller for Windows
# Usage: irm https://raw.githubusercontent.com/QSMxT/QSMxT/main/uninstall.ps1 | iex

$ErrorActionPreference = "Stop"

$installDir = "$env:USERPROFILE\.qsmxt\bin"
$binary = Join-Path $installDir "qsmxt.exe"

if (-not (Test-Path $binary)) {
    Write-Host "qsmxt not found at $binary"
    exit 0
}

Remove-Item -Force $binary
Write-Host "Removed $binary"

# Remove bundled dcm2niix if present
$dcm = Join-Path $installDir "dcm2niix.exe"
if (Test-Path $dcm) {
    Remove-Item -Force $dcm
    $dcmLicense = Join-Path $installDir "dcm2niix.LICENSE"
    if (Test-Path $dcmLicense) { Remove-Item -Force $dcmLicense }
    Write-Host "Removed bundled dcm2niix from $installDir"
}

# Remove install dir if empty
if ((Get-ChildItem $installDir -ErrorAction SilentlyContinue | Measure-Object).Count -eq 0) {
    Remove-Item -Recurse -Force "$env:USERPROFILE\.qsmxt"
    Write-Host "Removed $env:USERPROFILE\.qsmxt"
}

# Remove from PATH
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -like "*$installDir*") {
    $newPath = ($userPath -split ";" | Where-Object { $_ -ne $installDir }) -join ";"
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
    Write-Host "Removed $installDir from PATH (restart your terminal for this to take effect)."
}

Write-Host "qsmxt has been uninstalled."
