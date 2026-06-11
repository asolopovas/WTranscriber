#Requires -Version 5.1
<#
.SYNOPSIS
  Install NVIDIA cuDNN 9 (for CUDA 12) globally on Windows and add it to user PATH.

.DESCRIPTION
  WTranscriber's auto-installer normally fetches cuDNN on first start when
  device=cuda. This script does the same thing manually, useful if you want
  to install cuDNN ahead of time or repair a broken install.

  Installs to:  %LOCALAPPDATA%\Programs\cuDNN\v9
  Adds to user PATH:  %LOCALAPPDATA%\Programs\cuDNN\v9\bin

.PARAMETER Version
  cuDNN version to install. Defaults to 9.21.1.3.

.PARAMETER Force
  Reinstall even if cudnn64_9.dll is already present.

.EXAMPLE
  pwsh -File scripts/install-cudnn.ps1
  pwsh -File scripts/install-cudnn.ps1 -Version 9.9.0.52 -Force
#>

[CmdletBinding()]
param(
  [string]$Version = '9.21.1.3',
  [switch]$Force
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'Continue'

function Write-Step($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Write-Ok($msg)   { Write-Host "    $msg" -ForegroundColor Green }

if ($IsLinux -or $IsMacOS) {
  Write-Error 'This script is Windows-only.'
  exit 1
}

$installRoot = Join-Path $env:LOCALAPPDATA 'Programs\cuDNN\v9'
$installBin  = Join-Path $installRoot 'bin'
$cacheDir    = Join-Path $env:APPDATA 'asolopovas\wtranscriber\data\cache\cudnn'
$archive     = "cudnn-windows-x86_64-${Version}_cuda12-archive.zip"
$archiveUrl  = "https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/windows-x86_64/$archive"
$archivePath = Join-Path $cacheDir $archive
$stagingDir  = Join-Path $cacheDir 'staging'
$dllPath     = Join-Path $installBin 'cudnn64_9.dll'

if ((Test-Path $dllPath) -and -not $Force) {
  Write-Ok "cuDNN already installed at $dllPath"
  Write-Ok 'Use -Force to reinstall.'
} else {
  New-Item -ItemType Directory -Path $cacheDir -Force | Out-Null

  if (-not (Test-Path $archivePath)) {
    Write-Step "Downloading $archive (~700 MB)"
    Write-Host "    $archiveUrl"
    $tmpPath = "$archivePath.tmp"
    if (Test-Path $tmpPath) { Remove-Item -Force $tmpPath }
    Invoke-WebRequest -Uri $archiveUrl -OutFile $tmpPath -UseBasicParsing
    Move-Item -Force $tmpPath $archivePath
    Write-Ok "Cached: $archivePath"
  } else {
    Write-Ok "Using cached archive: $archivePath"
  }

  Write-Step 'Extracting'
  if (Test-Path $stagingDir) { Remove-Item -Recurse -Force $stagingDir }
  Expand-Archive -Path $archivePath -DestinationPath $stagingDir -Force

  $dll = Get-ChildItem $stagingDir -Recurse -Filter 'cudnn64_9.dll' | Select-Object -First 1
  if ($null -eq $dll) {
    Write-Error "Archive layout unexpected: no cudnn64_9.dll under $stagingDir"
    exit 1
  }

  Write-Step "Installing to $installRoot"
  if (Test-Path $installRoot) { Remove-Item -Recurse -Force $installRoot }
  New-Item -ItemType Directory -Path $installBin -Force | Out-Null
  # cuDNN <=9.x shipped DLLs in bin/; newer archives nest them in bin/x64/.
  # Normalise to $installBin so PATH and the app's lookup stay version-proof.
  Get-ChildItem $dll.DirectoryName -File | Move-Item -Destination $installBin
  Remove-Item -Recurse -Force $stagingDir

  if (-not (Test-Path $dllPath)) {
    Write-Error "Installation incomplete: $dllPath missing"
    exit 1
  }
  Write-Ok "Installed: $dllPath"
}

Write-Step 'Updating user PATH'
$currentPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (-not $currentPath) { $currentPath = '' }
$entries = $currentPath -split ';' | Where-Object { $_ -ne '' }
if ($entries -contains $installBin) {
  Write-Ok 'Already on user PATH.'
} else {
  $newPath = if ($currentPath -eq '') { $installBin } else { "$currentPath;$installBin" }
  [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
  Write-Ok "Added to user PATH: $installBin"
  Write-Ok 'Open a new shell (or restart WTranscriber) for PATH to take effect.'
}

Write-Step 'Done.'
