#Requires -Version 5.1
<#
.SYNOPSIS
  Install the prebuilt sherpa-onnx CUDA archive on Windows and add it to user PATH.

.DESCRIPTION
  Downloads the upstream sherpa-onnx CUDA-enabled Windows archive, extracts it
  under %LOCALAPPDATA%\Programs\sherpa-onnx-cuda\<version>, and exposes its
  lib directory via SHERPA_ONNX_LIB_DIR (build-time link) plus user PATH
  (runtime DLL load for `--features cuda` builds).

.PARAMETER Version
  sherpa-onnx tag to install. Defaults to v1.13.0.

.PARAMETER Force
  Reinstall even if sherpa-onnx-c-api.lib is already present.

.EXAMPLE
  pwsh -File scripts/install-sherpa-cuda.ps1
  pwsh -File scripts/install-sherpa-cuda.ps1 -Version v1.13.0 -Force
#>

[CmdletBinding()]
param(
  [string]$Version,
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

if (-not $Version) {
  $versionFile = Join-Path $PSScriptRoot '..\src-tauri\sherpa-version.txt'
  if (-not (Test-Path $versionFile)) {
    Write-Error "Version not provided and $versionFile not found."
    exit 1
  }
  $Version = (Get-Content $versionFile -Raw).Trim()
}

$stem        = "sherpa-onnx-$Version-cuda-12.x-cudnn-9.x-win-x64-cuda"
$installRoot = Join-Path $env:LOCALAPPDATA "Programs\sherpa-onnx-cuda\$Version"
$extractDir  = Join-Path $installRoot $stem
$libDir      = Join-Path $extractDir 'lib'
$binDir      = Join-Path $extractDir 'bin'
$cacheDir    = Join-Path $env:APPDATA 'asolopovas\wtranscriber\data\cache\sherpa-onnx-cuda'
$archive     = "$stem.tar.bz2"
$archiveUrl  = "https://github.com/k2-fsa/sherpa-onnx/releases/download/$Version/$archive"
$archivePath = Join-Path $cacheDir $archive
$libPath     = Join-Path $libDir 'sherpa-onnx-c-api.lib'
$dllPath     = Join-Path $binDir 'sherpa-onnx-c-api.dll'

if ((Test-Path $libPath) -and -not $Force) {
  Write-Ok "sherpa-onnx CUDA already installed at $libPath"
  Write-Ok 'Use -Force to reinstall.'
} else {
  if (-not (Get-Command tar -ErrorAction SilentlyContinue)) {
    Write-Error 'tar.exe not found on PATH'
    exit 1
  }

  New-Item -ItemType Directory -Path $cacheDir -Force | Out-Null
  New-Item -ItemType Directory -Path $installRoot -Force | Out-Null

  if (-not (Test-Path $archivePath)) {
    Write-Step "Downloading $archive"
    Write-Host "    $archiveUrl"
    Invoke-WebRequest -Uri $archiveUrl -OutFile $archivePath -UseBasicParsing
    Write-Ok "Cached: $archivePath"
  } else {
    Write-Ok "Using cached archive: $archivePath"
  }

  Write-Step "Extracting to $installRoot"
  tar -xjf $archivePath -C $installRoot

  if (-not (Test-Path $libPath)) {
    Write-Error "Installation incomplete: $libPath missing"
    exit 1
  }
  if (-not (Test-Path $dllPath)) {
    Write-Error "Installation incomplete: $dllPath missing"
    exit 1
  }
  Write-Ok "Installed: $libPath"
  Write-Ok "Installed: $dllPath"
}

Write-Step 'Setting SHERPA_ONNX_LIB_DIR'
[Environment]::SetEnvironmentVariable('SHERPA_ONNX_LIB_DIR', $libDir, 'User')
Write-Ok "SHERPA_ONNX_LIB_DIR=$libDir"

Write-Step 'Updating user PATH'
$currentPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (-not $currentPath) { $currentPath = '' }
$entries = $currentPath -split ';' | Where-Object { $_ -ne '' }
$toAdd = @($binDir) | Where-Object { $entries -notcontains $_ }
if (-not $toAdd) {
  Write-Ok 'Already on user PATH.'
} else {
  $newPath = (@($entries) + $toAdd) -join ';'
  [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
  foreach ($p in $toAdd) { Write-Ok "Added to user PATH: $p" }
}

Write-Step 'Done. Open a new shell before running `just build-cuda`.'
Write-Ok 'Reminder: CUDA 12.x runtime (cudart64_12.dll) and cuDNN 9 must also be installed.'
Write-Ok '         Run `just cudnn` for cuDNN. Install the CUDA Toolkit from https://developer.nvidia.com/cuda-downloads.'
