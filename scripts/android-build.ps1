[CmdletBinding()]
param(
    [ValidateSet('aarch64', 'armv7', 'i686', 'x86_64')]
    [string]$Target = 'aarch64',
    [switch]$DebugBuild
)

$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')

if (-not $env:ANDROID_HOME) {
    $env:ANDROID_HOME = Join-Path $env:LOCALAPPDATA 'Android\Sdk'
}
if (-not $env:NDK_HOME) {
    $env:NDK_HOME = Join-Path $env:ANDROID_HOME 'ndk\27.2.12479018'
}

$abiMap = @{
    'aarch64' = 'arm64-v8a'
    'armv7'   = 'armeabi-v7a'
    'i686'    = 'x86'
    'x86_64'  = 'x86_64'
}
$abi = $abiMap[$Target]
$prebuilt = Join-Path $root ".android-prebuilt\jniLibs\$abi"
if (-not (Test-Path (Join-Path $prebuilt 'libsherpa-onnx-c-api.so'))) {
    & (Join-Path $PSScriptRoot 'install-android-prebuilts.ps1')
}

$ndkBin = Join-Path $env:NDK_HOME 'toolchains\llvm\prebuilt\windows-x86_64\bin'
$env:SHERPA_ONNX_LIB_DIR = $prebuilt

$tripleMap = @{
    'aarch64' = @{ rust = 'aarch64_linux_android'; clang = 'aarch64-linux-android24-clang' }
    'armv7'   = @{ rust = 'armv7_linux_androideabi'; clang = 'armv7a-linux-androideabi24-clang' }
    'i686'    = @{ rust = 'i686_linux_android'; clang = 'i686-linux-android24-clang' }
    'x86_64'  = @{ rust = 'x86_64_linux_android'; clang = 'x86_64-linux-android24-clang' }
}
$t = $tripleMap[$Target]
$cc = Join-Path $ndkBin "$($t.clang).cmd"
$cxx = Join-Path $ndkBin "$($t.clang)++.cmd"
$ar = Join-Path $ndkBin 'llvm-ar.exe'

Set-Item "Env:CC_$($t.rust)" $cc
Set-Item "Env:CXX_$($t.rust)" $cxx
Set-Item "Env:AR_$($t.rust)" $ar
Set-Item "Env:CARGO_TARGET_$($t.rust.ToUpper())_LINKER" $cc

$args = @('run', 'tauri', 'android', 'build', '--apk', '--target', $Target)
if ($DebugBuild) { $args += '--debug' }

Push-Location $root
try {
    & bun @args
    if ($LASTEXITCODE -ne 0) { throw "tauri android build failed (exit $LASTEXITCODE)" }
} finally {
    Pop-Location
}

$apk = Join-Path $root 'src-tauri\gen\android\app\build\outputs\apk\universal'
$apk += if ($DebugBuild) { '\debug\app-universal-debug.apk' } else { '\release\app-universal-release-unsigned.apk' }
if (Test-Path $apk) {
    Write-Host ""
    Write-Host "APK: $apk" -ForegroundColor Green
    Write-Host "size: $((Get-Item $apk).Length / 1MB) MB"
}
