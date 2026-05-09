$ErrorActionPreference = 'Stop'
$ProgressPreference     = 'SilentlyContinue'

Write-Host '=== WTranscriber Windows build-host bootstrap ===' -ForegroundColor Cyan

if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
    throw 'winget unavailable. Install App Installer from the Microsoft Store, then re-run.'
}

function Have($cmd) { [bool](Get-Command $cmd -ErrorAction SilentlyContinue) }

function Add-Path($dir) {
    if (-not (Test-Path $dir)) { return }
    $cur = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    if ($cur -notlike "*$dir*") {
        [Environment]::SetEnvironmentVariable('Path', "$cur;$dir", 'Machine')
        Write-Host "  + PATH += $dir"
    }
    if ($env:Path -notlike "*$dir*") { $env:Path += ";$dir" }
}

function Winget-Install($id) {
    Write-Host "-> winget install $id"
    winget install --id $id --source winget --silent --accept-package-agreements --accept-source-agreements `
        --scope machine --disable-interactivity 2>&1 | Out-Host
    if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne -1978335189) { throw "winget failed for $id" }
}

$tmp = Join-Path $env:TEMP 'wtranscriber-bootstrap'
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

if (-not (Have rustup)) {
    Write-Host '-> Rust (rustup, gnu host)' -ForegroundColor Cyan
    $exe = "$tmp\rustup-init.exe"
    Invoke-WebRequest 'https://win.rustup.rs/x86_64' -OutFile $exe
    & $exe -y --default-host x86_64-pc-windows-gnu --default-toolchain stable --profile minimal
    Add-Path "$env:USERPROFILE\.cargo\bin"
}

if (-not (Have bun)) {
    Write-Host '-> Bun' -ForegroundColor Cyan
    Winget-Install 'Oven-sh.Bun'
    Add-Path "$env:USERPROFILE\.bun\bin"
}

if (-not (Have node)) {
    Write-Host '-> Node.js' -ForegroundColor Cyan
    Winget-Install 'OpenJS.NodeJS.LTS'
    Add-Path 'C:\Program Files\nodejs'
}

$nsis = 'C:\Program Files (x86)\NSIS\makensis.exe'
if (-not (Test-Path $nsis)) {
    Write-Host '-> NSIS' -ForegroundColor Cyan
    Winget-Install 'NSIS.NSIS'
    Add-Path 'C:\Program Files (x86)\NSIS'
}

if (-not (Have cmake)) {
    Write-Host '-> CMake (sherpa-onnx static needs it)' -ForegroundColor Cyan
    Winget-Install 'Kitware.CMake'
    Add-Path 'C:\Program Files\CMake\bin'
}

if (-not (Test-Path 'C:\msys64\mingw64\bin\gcc.exe')) {
    Write-Host '-> MSYS2 / MinGW-w64' -ForegroundColor Cyan
    Winget-Install 'MSYS2.MSYS2'
}
Add-Path 'C:\msys64\mingw64\bin'

if (-not (Have just)) {
    Write-Host '-> just (cross-platform task runner)' -ForegroundColor Cyan
    Winget-Install 'Casey.Just'
    Add-Path 'C:\Program Files\just'
    Add-Path "$env:USERPROFILE\.cargo\bin"
}

Write-Host '=== Done. Re-open shell or run `refreshenv` ===' -ForegroundColor Green
foreach ($t in 'just','rustup','rustc','cargo','bun','node','makensis','cmake','gcc') {
    if (Have $t) { Write-Host "  OK  $t" } else { Write-Host "  MISS $t" -ForegroundColor Yellow }
}
