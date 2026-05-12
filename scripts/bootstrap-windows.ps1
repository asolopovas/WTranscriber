$ErrorActionPreference = 'Stop'
$ProgressPreference     = 'SilentlyContinue'

Write-Host '=== WTranscriber Windows build-host bootstrap ===' -ForegroundColor Cyan

if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
    throw 'winget unavailable. Install App Installer from the Microsoft Store, then re-run.'
}

function Have($cmd) { [bool](Get-Command $cmd -ErrorAction SilentlyContinue) }

function Add-Path($dir) {
    if (-not (Test-Path $dir)) { return }
    $cur = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($cur -notlike "*$dir*") {
        $new = if ([string]::IsNullOrEmpty($cur)) { $dir } else { "$cur;$dir" }
        [Environment]::SetEnvironmentVariable('Path', $new, 'User')
        Write-Host "  + PATH += $dir (User)"
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

$vcvars = 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat'
if (-not (Test-Path $vcvars)) {
    Write-Host '-> Visual Studio 2022 Build Tools (VCTools workload)' -ForegroundColor Cyan
    Write-Host '   (large download, ~5 GB; required because ort prebuilt binaries are MSVC-only)'
    winget install --id Microsoft.VisualStudio.2022.BuildTools --silent --accept-package-agreements --accept-source-agreements `
        --override '--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --add Microsoft.VisualStudio.Component.Windows11SDK.22621 --includeRecommended' 2>&1 | Out-Host
    if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne -1978335189) { throw 'winget failed for Microsoft.VisualStudio.2022.BuildTools' }
}

if (-not (Have rustup)) {
    Write-Host '-> Rust (rustup, msvc host)' -ForegroundColor Cyan
    $exe = "$tmp\rustup-init.exe"
    Invoke-WebRequest 'https://win.rustup.rs/x86_64' -OutFile $exe
    & $exe -y --default-host x86_64-pc-windows-msvc --default-toolchain stable --profile minimal
    Add-Path "$env:USERPROFILE\.cargo\bin"
} else {
    & "$env:USERPROFILE\.cargo\bin\rustup.exe" set default-host x86_64-pc-windows-msvc | Out-Host
    & "$env:USERPROFILE\.cargo\bin\rustup.exe" default stable-x86_64-pc-windows-msvc | Out-Host
    & "$env:USERPROFILE\.cargo\bin\rustup.exe" target add x86_64-pc-windows-msvc | Out-Host
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
}
Add-Path 'C:\Program Files (x86)\NSIS'

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

if (-not (Have ninja)) {
    Write-Host '-> Ninja (CMAKE_GENERATOR=Ninja)' -ForegroundColor Cyan
    Winget-Install 'Ninja-build.Ninja'
    Add-Path "$env:LOCALAPPDATA\Microsoft\WinGet\Links"
}

$llvmBin = 'C:\Program Files\LLVM\bin'
if (-not (Test-Path (Join-Path $llvmBin 'libclang.dll'))) {
    Write-Host '-> LLVM (libclang for bindgen + lld-link for fast Rust linking)' -ForegroundColor Cyan
    Winget-Install 'LLVM.LLVM'
}
if (Test-Path $llvmBin) {
    [Environment]::SetEnvironmentVariable('LIBCLANG_PATH', $llvmBin, 'User')
    $env:LIBCLANG_PATH = $llvmBin
    Add-Path $llvmBin
}

if (-not (Have sccache)) {
    Write-Host '-> sccache (rustc + cmake C/C++ compile cache)' -ForegroundColor Cyan
    Winget-Install 'Mozilla.sccache'
    Add-Path "$env:LOCALAPPDATA\Microsoft\WinGet\Links"
}
$buildEnv = [ordered]@{}
if (Have sccache) {
    $buildEnv['RUSTC_WRAPPER']              = 'sccache'
    $buildEnv['CMAKE_C_COMPILER_LAUNCHER']  = 'sccache'
    $buildEnv['CMAKE_CXX_COMPILER_LAUNCHER']= 'sccache'
}
if (Test-Path (Join-Path $llvmBin 'lld-link.exe')) {
    $buildEnv['CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER'] = 'lld-link.exe'
}
foreach ($k in $buildEnv.Keys) {
    $val = $buildEnv[$k]
    if ([Environment]::GetEnvironmentVariable($k, 'User') -ne $val) {
        [Environment]::SetEnvironmentVariable($k, $val, 'User')
        Write-Host "  + $k=$val (User)"
    }
    Set-Item -Path "env:$k" -Value $val
}

# Pin to CUDA 12.x: whisper-rs-sys + parakeet-rs (ggml) + the bundled
# sherpa-onnx-cuda runtime are all built against CUDA 12.x / cuDNN 9. CUDA 13
# bumps the ABI and won't link.
$cudaWanted = '12.9'
function Find-Cuda12 {
    Get-ChildItem 'C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA' -ErrorAction SilentlyContinue |
        Where-Object {
            $_.Name -match '^v12\.' -and (Test-Path (Join-Path $_.FullName 'bin\nvcc.exe'))
        } |
        Sort-Object Name -Descending | Select-Object -First 1
}
$cuda12 = Find-Cuda12
if (-not $cuda12) {
    Write-Host "-> CUDA Toolkit $cudaWanted (whisper-rs-sys + parakeet-rs cuda)" -ForegroundColor Cyan
    Write-Host '   (large download, ~3 GB; CUDA 13 is ABI-incompatible with the bundled sherpa-cuda runtime)'
    # --force lets winget install 12.x side-by-side when 13.x is already
    # registered (winget otherwise tries to upgrade and refuses).
    winget install --id Nvidia.CUDA --version $cudaWanted --source winget --silent `
        --accept-package-agreements --accept-source-agreements --scope machine `
        --disable-interactivity --force 2>&1 | Out-Host
    if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne -1978335189) {
        Write-Host "   WARN  winget failed for Nvidia.CUDA $cudaWanted (exit $LASTEXITCODE)" -ForegroundColor Yellow
    }
    $cuda12 = Find-Cuda12
}
if ($cuda12) {
    $cudaRoot = $cuda12.FullName
    [Environment]::SetEnvironmentVariable('CUDA_PATH', $cudaRoot, 'User')
    $env:CUDA_PATH = $cudaRoot
    Add-Path (Join-Path $cudaRoot 'bin')
    Write-Host "   CUDA_PATH -> $cudaRoot" -ForegroundColor Green
} else {
    Write-Host '   WARN  CUDA 12.x install did not land; `just build` (default cuda feature) will fail.' -ForegroundColor Yellow
    Write-Host '         Install manually from https://developer.nvidia.com/cuda-12-9-1-download-archive' -ForegroundColor Yellow
    Write-Host '         or build with --no-default-features --features sherpa-static.' -ForegroundColor Yellow
}

$cudnnDll = Join-Path $env:LOCALAPPDATA 'Programs\cuDNN\v9\bin\cudnn64_9.dll'
$sysDll   = 'C:\Windows\System32\cudnn64_9.dll'
if (-not ((Test-Path $cudnnDll) -or (Test-Path $sysDll))) {
    Write-Host '-> cuDNN 9 (CUDA 12)' -ForegroundColor Cyan
    & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'install-cudnn.ps1')
}

$sherpaVerFile = Join-Path $PSScriptRoot '..\src-tauri\sherpa-version.txt'
if (Test-Path $sherpaVerFile) {
    $sherpaVer = (Get-Content $sherpaVerFile -Raw).Trim()
    $sherpaLib = Join-Path $env:LOCALAPPDATA "Programs\sherpa-onnx-cuda\$sherpaVer"
    if (-not (Test-Path (Join-Path $sherpaLib '*\lib\sherpa-onnx-c-api.lib'))) {
        Write-Host '-> sherpa-onnx CUDA runtime' -ForegroundColor Cyan
        & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'install-sherpa-cuda.ps1')
    }
}

Write-Host '=== Done. Re-open shell or run `refreshenv` ===' -ForegroundColor Green
foreach ($t in 'just','rustup','rustc','cargo','bun','node','makensis','cmake','ninja','sccache','lld-link') {
    if (Have $t) { Write-Host "  OK  $t" } else { Write-Host "  MISS $t" -ForegroundColor Yellow }
}
if (Test-Path (Join-Path $llvmBin 'libclang.dll')) { Write-Host "  OK  libclang" } else { Write-Host "  MISS libclang" -ForegroundColor Yellow }
if ($cudaRoot -and (Test-Path (Join-Path $cudaRoot 'bin\nvcc.exe'))) { Write-Host "  OK  nvcc ($cudaRoot)" } else { Write-Host "  MISS nvcc" -ForegroundColor Yellow }
