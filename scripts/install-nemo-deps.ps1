#Requires -Version 5.1
<#
.SYNOPSIS
  Install Python dependencies for NeMo Sortformer diarization.

.DESCRIPTION
  Resolves the same Python interpreter `wtranscriber` uses at runtime
  (WT_PYTHON env var, then %APPDATA%\asolopovas\wtranscriber\data\python\,
  then python on PATH) and installs `nemo_toolkit[asr]` into it via uv.

  After this finishes, the `auto` and `nemo` diarizer choices will work.
#>

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

function Write-Step($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Write-Ok($msg)   { Write-Host "    $msg" -ForegroundColor Green }

if (-not (Get-Command uv -ErrorAction SilentlyContinue)) {
  Write-Error "uv not found on PATH. Install from https://docs.astral.sh/uv/getting-started/installation/"
  exit 1
}

$python = $null
if ($env:WT_PYTHON -and (Test-Path $env:WT_PYTHON)) {
  $python = $env:WT_PYTHON
} else {
  $bundled = Join-Path $env:APPDATA 'asolopovas\wtranscriber\data\python\Scripts\python.exe'
  if (Test-Path $bundled) {
    $python = $bundled
  } elseif (Get-Command python -ErrorAction SilentlyContinue) {
    $python = (Get-Command python).Source
  } elseif (Get-Command python3 -ErrorAction SilentlyContinue) {
    $python = (Get-Command python3).Source
  }
}

if (-not $python) {
  Write-Error "No Python found. Install Python 3.10+ or set WT_PYTHON to a usable interpreter."
  exit 1
}

Write-Step "Installing nemo_toolkit[asr] into $python"
uv pip install --python "$python" 'nemo_toolkit[asr]'

Write-Step 'Verifying import'
& "$python" -c "import nemo.collections.asr; print('nemo.collections.asr OK')"

Write-Ok 'NeMo dependencies ready. The `auto` and `nemo` diarizer choices will work.'
