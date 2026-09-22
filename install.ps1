# QSMxT installer for Windows
# Usage: irm https://raw.githubusercontent.com/QSMxT/QSMxT/main/install.ps1 | iex

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

# Get latest release tag
Write-Host "Fetching latest release..."
$headers = @{}
if ($env:GITHUB_TOKEN) {
    $headers["Authorization"] = "token $env:GITHUB_TOKEN"
}
$release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest" -Headers $headers
$tag = $release.tag_name

if (-not $tag) {
    Write-Error "Could not determine latest release"
    exit 1
}

Write-Host "Installing qsmxt $tag for Windows..."

# Download
$url = "https://github.com/$repo/releases/download/$tag/qsmxt-$tag-$target.zip"
$tmpZip = Join-Path $env:TEMP "qsmxt.zip"
$tmpDir = Join-Path $env:TEMP "qsmxt-extract"

Save-FileWithProgress -Uri $url -OutFile $tmpZip -Activity "Downloading qsmxt $tag"

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
