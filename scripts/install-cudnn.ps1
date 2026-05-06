#Requires -Version 5.1
<#
.SYNOPSIS
  Install NVIDIA cuDNN 9 (for CUDA 12) DLLs alongside the sherpa-onnx CUDA runtime.

.DESCRIPTION
  The sherpa-onnx CUDA build is linked against cuDNN 9.x but does not bundle
  cudnn64_9.dll. This script downloads the public cuDNN 9 redistributable
  archive from NVIDIA and copies its bin DLLs next to sherpa-onnx-offline.exe
  inside the WTranscriber data directory.

  The downloaded archive is cached so re-running is cheap.

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
function Write-Warn2($msg) { Write-Host "    $msg" -ForegroundColor Yellow }

if ($IsLinux -or $IsMacOS) {
  Write-Error 'This script is Windows-only. cuDNN on Linux/macOS is installed via your package manager.'
  exit 1
}

$dataRoot   = Join-Path $env:APPDATA 'asolopovas\wtranscriber\data'
$cudaBin    = Join-Path $dataRoot 'third_party\sherpa-onnx\v1.13.0-cuda\bin'
$cacheDir   = Join-Path $dataRoot 'cache\cudnn'
$archive    = "cudnn-windows-x86_64-${Version}_cuda12-archive.zip"
$archiveUrl = "https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/windows-x86_64/$archive"
$archivePath = Join-Path $cacheDir $archive
$extractDir  = Join-Path $cacheDir ([IO.Path]::GetFileNameWithoutExtension($archive))

if (-not (Test-Path $cudaBin)) {
  Write-Error @"
sherpa-onnx CUDA runtime not found at:
  $cudaBin

Run the WTranscriber app once with device=cuda so the runtime is auto-installed,
then re-run this script.
"@
  exit 1
}

$cudnnTarget = Join-Path $cudaBin 'cudnn64_9.dll'
if ((Test-Path $cudnnTarget) -and -not $Force) {
  Write-Ok "cuDNN already installed at $cudnnTarget"
  Write-Ok 'Use -Force to reinstall.'
  exit 0
}

New-Item -ItemType Directory -Path $cacheDir -Force | Out-Null

if (-not (Test-Path $archivePath)) {
  Write-Step "Downloading $archive (~700 MB)"
  Write-Host "    $archiveUrl"
  try {
    Invoke-WebRequest -Uri $archiveUrl -OutFile $archivePath -UseBasicParsing
  } catch {
    Write-Error "Download failed: $($_.Exception.Message)`nVerify version $Version exists at:`nhttps://developer.download.nvidia.com/compute/cudnn/redist/cudnn/windows-x86_64/"
    exit 1
  }
  Write-Ok "Cached: $archivePath"
} else {
  Write-Ok "Using cached archive: $archivePath"
}

Write-Step 'Extracting archive'
if (Test-Path $extractDir) { Remove-Item -Recurse -Force $extractDir }
Expand-Archive -Path $archivePath -DestinationPath $cacheDir -Force
if (-not (Test-Path $extractDir)) {
  $candidate = Get-ChildItem $cacheDir -Directory | Where-Object { $_.Name -like 'cudnn-windows-x86_64-*' } | Select-Object -First 1
  if ($null -ne $candidate) { $extractDir = $candidate.FullName }
}

$srcBin = Join-Path $extractDir 'bin'
if (-not (Test-Path $srcBin)) {
  Write-Error "Archive layout unexpected: no bin/ inside $extractDir"
  exit 1
}

Write-Step "Copying cuDNN DLLs to $cudaBin"
$copied = 0
Get-ChildItem -Path $srcBin -Filter '*.dll' | ForEach-Object {
  $dst = Join-Path $cudaBin $_.Name
  Copy-Item -Path $_.FullName -Destination $dst -Force
  $copied++
}
Write-Ok "Copied $copied DLL(s)"

if (-not (Test-Path $cudnnTarget)) {
  Write-Error "Installation incomplete: $cudnnTarget missing"
  exit 1
}

Write-Step 'Done.'
Write-Ok "cudnn64_9.dll -> $cudnnTarget"
Write-Ok 'Restart WTranscriber. Transcription will now use the GPU.'
