[CmdletBinding()]
param(
    [string]$Version = (Get-Content (Join-Path $PSScriptRoot '..\src-tauri\sherpa-version.txt') -Raw).Trim().TrimStart('v')
)

$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
$dest = Join-Path $root '.android-prebuilt'
$archive = "sherpa-onnx-v$Version-android.tar.bz2"
$url = "https://github.com/k2-fsa/sherpa-onnx/releases/download/v$Version/$archive"
$archivePath = Join-Path $dest $archive
$marker = Join-Path $dest "jniLibs\arm64-v8a\libsherpa-onnx-c-api.so"

if (Test-Path $marker) {
    Write-Host "android prebuilts already present at $dest"
    exit 0
}

New-Item -ItemType Directory -Force -Path $dest | Out-Null
if (-not (Test-Path $archivePath)) {
    Write-Host "downloading $url"
    Invoke-WebRequest -Uri $url -OutFile $archivePath
}

Write-Host "extracting $archive"
tar -xjf $archivePath -C $dest
Set-Content -Path (Join-Path $dest '.gitignore') -Value '*'
Write-Host "android prebuilts staged at $dest"
