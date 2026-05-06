#Requires -Version 5.1
<#
.SYNOPSIS
  Install Python dependencies for NeMo Sortformer diarization.

.DESCRIPTION
  Provisions a managed Python 3.12 venv at the location wtranscriber probes
  at runtime (%APPDATA%\asolopovas\wtranscriber\data\python\) and installs
  nemo_toolkit[asr] into it via uv.

  Resolution order (matches src-tauri/src/diarizer/nemo.rs::resolve_python):
    1. $env:WT_PYTHON (if set and exists)
    2. <data>\python\Scripts\python.exe        <-- created by this script
    3. python / python3 on PATH (only if 3.10–3.12)

  After this finishes, the `auto` and `nemo` diarizer choices will work.
#>

[CmdletBinding()]
param(
  [string]$PythonVersion = '3.12'
)

$ErrorActionPreference = 'Stop'

function Write-Step($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Write-Ok($msg)   { Write-Host "    $msg" -ForegroundColor Green }
function Write-Warn2($msg){ Write-Host "    $msg" -ForegroundColor Yellow }

if (-not (Get-Command uv -ErrorAction SilentlyContinue)) {
  Write-Error "uv not found on PATH. Install from https://docs.astral.sh/uv/getting-started/installation/"
  exit 1
}

$dataRoot = Join-Path $env:APPDATA 'asolopovas\wtranscriber\data'
$venvDir  = Join-Path $dataRoot 'python'
$venvPy   = Join-Path $venvDir 'Scripts\python.exe'

function Test-CompatiblePython($exe) {
  if (-not $exe -or -not (Test-Path $exe)) { return $false }
  try {
    $out = & $exe -c "import sys; print('%d.%d' % sys.version_info[:2])" 2>$null
    if (-not $out) { return $false }
    $parts = $out.Trim().Split('.')
    $maj = [int]$parts[0]; $min = [int]$parts[1]
    return ($maj -eq 3 -and $min -ge 10 -and $min -le 12)
  } catch { return $false }
}

$python = $null

if ($env:WT_PYTHON -and (Test-CompatiblePython $env:WT_PYTHON)) {
  Write-Step "Using WT_PYTHON: $($env:WT_PYTHON)"
  $python = $env:WT_PYTHON
} elseif (Test-CompatiblePython $venvPy) {
  Write-Step "Using existing managed venv: $venvPy"
  $python = $venvPy
} else {
  if (Test-Path $venvDir) {
    Write-Warn2 "Existing $venvDir is not a compatible Python 3.10–3.12; recreating"
    Remove-Item -Recurse -Force $venvDir
  }
  Write-Step "Creating managed venv (Python $PythonVersion) at $venvDir"
  uv venv --python $PythonVersion $venvDir
  if (-not (Test-Path $venvPy)) {
    Write-Error "Failed to create venv at $venvDir"
    exit 1
  }
  $python = $venvPy
}

Write-Step "Installing nemo_toolkit[asr] into $python"
uv pip install --python "$python" 'nemo_toolkit[asr]'

Write-Step 'Verifying import'
& "$python" -c "import nemo.collections.asr; print('nemo.collections.asr OK')"

Write-Ok "NeMo dependencies ready at $python"
Write-Ok 'The `auto` and `nemo` diarizer choices will work.'
