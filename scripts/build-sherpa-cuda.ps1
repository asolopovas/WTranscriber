[CmdletBinding()]
param(
    [string]$Version = "v1.13.0",
    [string]$RepoUrl = "https://github.com/k2-fsa/sherpa-onnx.git",
    [string]$BuildRoot
)

$ErrorActionPreference = "Stop"

if (-not $BuildRoot -or $BuildRoot -eq "") {
    $BuildRoot = Join-Path $env:USERPROFILE ".wtranscriber\sherpa-onnx-cuda"
}

Write-Host "sherpa-onnx CUDA build" -ForegroundColor Cyan
Write-Host "  version : $Version"
Write-Host "  build   : $BuildRoot"
Write-Host ""

if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    throw "git not found on PATH"
}
if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) {
    throw "cmake not found on PATH"
}
if (-not $env:CUDA_PATH -or -not (Test-Path $env:CUDA_PATH)) {
    throw "CUDA_PATH is not set or invalid; install the NVIDIA CUDA Toolkit"
}

$src = Join-Path $BuildRoot "src"
$build = Join-Path $BuildRoot "build"
$install = Join-Path $BuildRoot "install"

if (-not (Test-Path $BuildRoot)) {
    New-Item -ItemType Directory -Path $BuildRoot | Out-Null
}

if (-not (Test-Path (Join-Path $src ".git"))) {
    Write-Host "Cloning $RepoUrl @ $Version into $src ..."
    git clone --depth 1 --branch $Version $RepoUrl $src
}
else {
    Write-Host "Updating $src ..."
    git -C $src fetch --depth 1 origin $Version
    git -C $src checkout $Version
}

if (Test-Path $build) {
    Remove-Item -Recurse -Force $build
}
New-Item -ItemType Directory -Path $build | Out-Null

Push-Location $build
try {
    cmake -S $src -B $build `
        -DCMAKE_BUILD_TYPE=Release `
        -DCMAKE_INSTALL_PREFIX="$install" `
        -DSHERPA_ONNX_ENABLE_GPU=ON `
        -DBUILD_SHARED_LIBS=ON `
        -DSHERPA_ONNX_ENABLE_PYTHON=OFF `
        -DSHERPA_ONNX_ENABLE_TESTS=OFF `
        -DSHERPA_ONNX_ENABLE_CHECK=OFF `
        -DSHERPA_ONNX_BUILD_C_API_EXAMPLES=OFF
    cmake --build $build --config Release --target install --parallel
}
finally {
    Pop-Location
}

Write-Host ""
Write-Host "Built CUDA-enabled sherpa-onnx in $install" -ForegroundColor Green
Write-Host ""
Write-Host "Set the following environment variables for cargo build:" -ForegroundColor Yellow
Write-Host "  setx SHERPA_ONNX_LIB_DIR `"$install\lib`""
Write-Host "  setx PATH `"$install\bin;%PATH%`""
Write-Host ""
Write-Host "Then in src-tauri/Cargo.toml swap the sherpa-onnx feature:" -ForegroundColor Yellow
Write-Host "  features = [\"shared\"]"
