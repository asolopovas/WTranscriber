param(
  [int]$StaleSec = 180
)
$ErrorActionPreference = 'Stop'

$root = Join-Path $env:TEMP 'pi-subagents-user-asolo\async-subagent-runs'
if (-not (Test-Path $root)) {
  Write-Host "no async runs directory at $root"
  exit 0
}

$resultsDir = Join-Path $env:TEMP 'pi-subagents-user-asolo\async-subagent-results'
$now = Get-Date

$rows = Get-ChildItem $root -Directory | ForEach-Object {
  $id = $_.Name
  $hasResult = Test-Path (Join-Path $resultsDir "$id.json")
  $events = Join-Path $_.FullName 'events.jsonl'
  $mtime = if (Test-Path $events) { (Get-Item $events).LastWriteTime } else { $_.LastWriteTime }
  $ageSec = [int]($now - $mtime).TotalSeconds
  $state = if ($hasResult) { 'done' }
           elseif ($ageSec -gt $StaleSec) { 'STALE' }
           else { 'live' }
  [pscustomobject]@{
    id    = $id.Substring(0, 8)
    state = $state
    age_s = $ageSec
    mtime = $mtime.ToString('HH:mm:ss')
  }
} | Sort-Object age_s

if (-not $rows) { Write-Host "no async runs"; exit 0 }
$rows | Format-Table -AutoSize

$stale = $rows | Where-Object { $_.state -eq 'STALE' }
if ($stale) {
  Write-Host ""
  Write-Host "STALE runs (>$StaleSec s without activity):"
  $stale | ForEach-Object { Write-Host "  subagent action: interrupt id: $($_.id)" }
  exit 2
}
exit 0
