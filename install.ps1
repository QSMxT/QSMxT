# QSMxT installer for Windows
# Usage: irm https://raw.githubusercontent.com/QSMxT/QSMxT/main/install.ps1 | iex
#
# The archive is verified against the release's SHA256SUMS before anything is
# extracted; a missing manifest, a missing entry or a mismatch aborts the install.
#
# Environment:
#   QSMXT_VERSION        install this release tag (e.g. v9.23.0) instead of the latest
#   QSMXT_DOWNLOAD_URL   base URL holding <tag>/<archive> and <tag>/SHA256SUMS
#                        (default: the GitHub release downloads; for mirrors and tests)

$ErrorActionPreference = "Stop"

# Stream a download to disk with a progress bar. Invoke-WebRequest's own progress
# rendering makes downloads dramatically slower on Windows PowerShell 5.1, so we
# suppress it and report progress ourselves.
function Save-FileWithProgress {
    param(
        [Parameter(Mandatory = $true)][string]$Uri,
        [Parameter(Mandatory = $true)][string]$OutFile,
        [string]$Activity = "Downloading"
    )

    try { Add-Type -AssemblyName System.Net.Http } catch { }
    if (-not ("System.Net.Http.HttpClient" -as [type])) {
        # No HttpClient to stream from; fall back to a plain download.
        Write-Host "$Activity..."
        Invoke-WebRequest -Uri $Uri -OutFile $OutFile
        return
    }

    $previousProgress = $ProgressPreference
    $ProgressPreference = "SilentlyContinue"
    try {
        $client = [System.Net.Http.HttpClient]::new()
        $client.Timeout = [TimeSpan]::FromMinutes(30)
        try {
            $response = $client.GetAsync($Uri, [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult()
            $response.EnsureSuccessStatusCode() | Out-Null
            $total = $response.Content.Headers.ContentLength

            $stream = $response.Content.ReadAsStreamAsync().GetAwaiter().GetResult()
            $output = [System.IO.File]::Create($OutFile)
            try {
                $buffer = New-Object byte[] 81920
                $downloaded = 0
                $lastReport = 0
                while (($read = $stream.Read($buffer, 0, $buffer.Length)) -gt 0) {
                    $output.Write($buffer, 0, $read)
                    $downloaded += $read
                    # Report at most once per MB so the bar doesn't dominate the transfer.
                    if (($downloaded - $lastReport) -ge 1MB) {
                        $lastReport = $downloaded
                        if ($total) {
                            Write-Progress -Activity $Activity `
                                -Status ("{0:N1} MB of {1:N1} MB" -f ($downloaded / 1MB), ($total / 1MB)) `
                                -PercentComplete ([int](100 * $downloaded / $total))
                        } else {
                            Write-Progress -Activity $Activity -Status ("{0:N1} MB" -f ($downloaded / 1MB))
                        }
                    }
                }
            } finally {
                $output.Dispose()
                $stream.Dispose()
                $response.Dispose()
            }
        } finally {
            $client.Dispose()
        }
        Write-Progress -Activity $Activity -Completed
    } finally {
        $ProgressPreference = $previousProgress
    }
}

$repo = "QSMxT/QSMxT"
$target = "x86_64-pc-windows-msvc"

# Default install directory
$installDir = "$env:USERPROFILE\.qsmxt\bin"

$downloadBase = if ($env:QSMXT_DOWNLOAD_URL) { $env:QSMXT_DOWNLOAD_URL } else { "https://github.com/$repo/releases/download" }

# Get latest release tag
$tag = $env:QSMXT_VERSION
if (-not $tag) {
    Write-Host "Fetching latest release..."
    $headers = @{}
    if ($env:GITHUB_TOKEN) {
        $headers["Authorization"] = "token $env:GITHUB_TOKEN"
    }
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest" -Headers $headers
    $tag = $release.tag_name
}

if (-not $tag) {
    Write-Error "Could not determine latest release"
    exit 1
}

Write-Host "Installing qsmxt $tag for Windows..."

# Download and verify
$asset = "qsmxt-$tag-$target.zip"
$url = "$downloadBase/$tag/$asset"
$tmpZip = Join-Path $env:TEMP "qsmxt.zip"
$tmpDir = Join-Path $env:TEMP "qsmxt-extract"

try {
    $sums = (Invoke-WebRequest -UseBasicParsing -Uri "$downloadBase/$tag/SHA256SUMS").Content
    if ($sums -is [byte[]]) { $sums = [System.Text.Encoding]::UTF8.GetString($sums) }
} catch {
    Write-Error "Could not download SHA256SUMS for $tag; refusing to install an unverified binary ($_)"
    exit 1
}
# Exact file-name match on the second field ("<hex>  <file>" or "<hex> *<file>").
$expected = $null
foreach ($line in ($sums -split "`r?`n")) {
    $fields = $line.Trim() -split '\s+', 2
    if ($fields.Count -eq 2 -and ($fields[1] -eq $asset -or $fields[1] -eq "*$asset")) {
        $expected = $fields[0].ToLowerInvariant()
        break
    }
}
if (-not $expected) {
    Write-Error "SHA256SUMS for $tag has no entry for $asset; refusing to install an unverified binary"
    exit 1
}

Save-FileWithProgress -Uri $url -OutFile $tmpZip -Activity "Downloading qsmxt $tag"

$actual = (Get-FileHash -Algorithm SHA256 -Path $tmpZip).Hash.ToLowerInvariant()
if ($actual -ne $expected) {
    Remove-Item -Force $tmpZip
    Write-Error "Checksum mismatch for ${asset}: SHA256SUMS says $expected, the download is $actual. Nothing was installed."
    exit 1
}
Write-Host "Verified $asset against SHA256SUMS."

# Extract
if (Test-Path $tmpDir) { Remove-Item -Recurse -Force $tmpDir }
Expand-Archive -Path $tmpZip -DestinationPath $tmpDir

# Install
if (-not (Test-Path $installDir)) {
    New-Item -ItemType Directory -Path $installDir -Force | Out-Null
}
Copy-Item -Path (Join-Path $tmpDir "qsmxt.exe") -Destination (Join-Path $installDir "qsmxt.exe") -Force

# Install bundled dcm2niix (present for x86_64 Windows; ARM builds rely on PATH)
$dcm = Join-Path $tmpDir "dcm2niix.exe"
if (Test-Path $dcm) {
    Copy-Item -Path $dcm -Destination (Join-Path $installDir "dcm2niix.exe") -Force
    $dcmLicense = Join-Path $tmpDir "dcm2niix.LICENSE"
    if (Test-Path $dcmLicense) {
        Copy-Item -Path $dcmLicense -Destination (Join-Path $installDir "dcm2niix.LICENSE") -Force
    }
    Write-Host "Installed bundled dcm2niix to $installDir\dcm2niix.exe"
}

# Clean up
Remove-Item -Force $tmpZip
Remove-Item -Recurse -Force $tmpDir

# Add to PATH if not already there
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$installDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$installDir", "User")
    Write-Host "Added $installDir to your PATH (restart your terminal for this to take effect)."
}

Write-Host ""
Write-Host "Installed qsmxt $tag to $installDir\qsmxt.exe"
Write-Host ""
Write-Host "Run 'qsmxt --version' to verify, or 'qsmxt tui' to get started."
