param(
  [Parameter(Mandatory=$true)][ValidateSet('desktop','android-usb','android-host')][string]$Platform
)

$ErrorActionPreference = 'Stop'
$Repo = Resolve-Path "$PSScriptRoot\.."
Set-Location $Repo
New-Item -ItemType Directory -Force -Path "$Repo\tmp" | Out-Null

git config core.hooksPath .githooks | Out-Null

function Spawn($file, $argList, $stdoutLog, $stderrLog) {
  $p = Start-Process -FilePath $file -ArgumentList $argList `
    -RedirectStandardOutput $stdoutLog -RedirectStandardError $stderrLog `
    -WorkingDirectory $Repo -WindowStyle Hidden -PassThru
  return $p.Id
}

function PortOwner($port) {
  $line = (& netstat -ano | Select-String ":$port\s+.*LISTENING" | Select-Object -First 1).ToString()
  if ($line -match '\s(\d+)\s*$') { return [int]$Matches[1] } else { return $null }
}

function Wait-ForPort($port, $timeoutSec = 90) {
  $deadline = (Get-Date).AddSeconds($timeoutSec)
  while ((Get-Date) -lt $deadline) {
    $pid = PortOwner $port
    if ($pid) { return $pid }
    Start-Sleep -Milliseconds 750
  }
  return $null
}

function Wait-ForLogLine($path, $pattern, $timeoutSec = 60) {
  $deadline = (Get-Date).AddSeconds($timeoutSec)
  while ((Get-Date) -lt $deadline) {
    if (Test-Path $path) {
      if (Select-String -Path $path -Pattern $pattern -Quiet -ErrorAction SilentlyContinue) { return $true }
    }
    Start-Sleep -Milliseconds 750
  }
  return $false
}

$pids = @{}

if ($Platform -eq 'desktop') {
  Set-Content -Path "$Repo\tmp\_platform" -Value 'desktop' -NoNewline
  $pids.dev_wrapper = Spawn 'just' @('dev') "$Repo\tmp\dev.log" "$Repo\tmp\dev.err.log"
  $owner = Wait-ForPort 1420 120
  if (-not $owner) { Write-Error "vite did not bind :1420 within 120s"; exit 1 }
  $pids.dev_port_owner = $owner
  $pids.error_monitor = Spawn 'node' @('scripts/error-monitor.mjs') "$Repo\tmp\error-monitor.log" "$Repo\tmp\error-monitor.err.log"
}
else {
  Set-Content -Path "$Repo\tmp\_platform" -Value 'android' -NoNewline
  & adb logcat -c | Out-Null
  $pids.logcat = Spawn 'adb' @('logcat','-b','main,events','*:W','RustStdoutStderr:V','Tauri:V','chromium:V','am_crash:V','am_proc_died:V','am_kill:V') "$Repo\tmp\logcat.log" "$Repo\tmp\logcat.err.log"

  $cmdPath = "$Repo\tmp\_dev.cmd"
  if ($Platform -eq 'android-usb') {
    "set TAURI_DEV_HOST=127.0.0.1 && just android-dev" | Set-Content -Path $cmdPath -NoNewline
    & adb reverse tcp:1420 tcp:1420 | Out-Null
    & adb reverse tcp:1421 tcp:1421 | Out-Null
  } else {
    "just android-dev-host" | Set-Content -Path $cmdPath -NoNewline
  }
  $pids.dev_wrapper = Spawn 'cmd.exe' @('/c', $cmdPath) "$Repo\tmp\android-dev.log" "$Repo\tmp\android-dev.err.log"

  $owner = Wait-ForPort 1420 180
  if (-not $owner) { Write-Error "vite did not bind :1420 within 180s"; exit 1 }
  $pids.dev_port_owner = $owner

  $hmrUp = Wait-ForLogLine "$Repo\tmp\android-dev.log" 'connecting to 127\.0\.0\.1:1420|connected to 127\.0\.0\.1:1420' 180
  if (-not $hmrUp) { Write-Error "WebView never connected to Vite — check adb reverse / TAURI_DEV_HOST"; exit 1 }

  & just android-debug-attach | Out-Null
}

$pids | ConvertTo-Json | Set-Content -Path "$Repo\tmp\_pids.json" -NoNewline
Write-Host "BOOTSTRAP OK platform=$Platform pids=$($pids | ConvertTo-Json -Compress)"
