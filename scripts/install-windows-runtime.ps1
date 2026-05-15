param(
    [Parameter(Mandatory = $true)]
    [string]$InstallDir
)

$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$cache = Join-Path $env:TEMP 'wtranscriber-runtime'
$licenses = Join-Path $InstallDir 'licenses'
New-Item -ItemType Directory -Force -Path $cache, $licenses | Out-Null

function Download-FileChecked([string]$Url, [string]$OutFile) {
    if ((Test-Path $OutFile) -and ((Get-Item $OutFile).Length -gt 0)) {
        return
    }
    $tmp = "$OutFile.tmp"
    Remove-Item -Force -ErrorAction SilentlyContinue $tmp
    Invoke-WebRequest -Uri $Url -OutFile $tmp -UseBasicParsing
    Move-Item -Force $tmp $OutFile
}

function Reset-Dir([string]$Path) {
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $Path
    New-Item -ItemType Directory -Force -Path $Path | Out-Null
}

function Expand-Zip([string]$Archive, [string]$Destination) {
    Reset-Dir $Destination
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [System.IO.Compression.ZipFile]::ExtractToDirectory($Archive, $Destination)
}

function Copy-IfExists([string]$Source, [string]$Destination) {
    if (Test-Path $Source) {
        Copy-Item -Force $Source $Destination
    }
}

function Install-OnnxRuntimeDirectML {
    $version = '1.24.2'
    $archive = Join-Path $cache "Microsoft.ML.OnnxRuntime.DirectML.$version.zip"
    $stage = Join-Path $cache "Microsoft.ML.OnnxRuntime.DirectML.$version"
    Download-FileChecked "https://www.nuget.org/api/v2/package/Microsoft.ML.OnnxRuntime.DirectML/$version" $archive
    Expand-Zip $archive $stage
    $native = Join-Path $stage 'runtimes\win-x64\native'
    Copy-Item -Force (Join-Path $native 'onnxruntime.dll') (Join-Path $InstallDir 'onnxruntime.dll')
    Copy-Item -Force (Join-Path $native 'onnxruntime_providers_shared.dll') (Join-Path $InstallDir 'onnxruntime_providers_shared.dll')
    Copy-IfExists (Join-Path $stage 'LICENSE') (Join-Path $licenses 'Microsoft.ML.OnnxRuntime.DirectML-LICENSE')
    Copy-IfExists (Join-Path $stage 'ThirdPartyNotices.txt') (Join-Path $licenses 'Microsoft.ML.OnnxRuntime.DirectML-ThirdPartyNotices.txt')
}

function Install-DirectML {
    $version = '1.15.4'
    $archive = Join-Path $cache "Microsoft.AI.DirectML.$version.zip"
    $stage = Join-Path $cache "Microsoft.AI.DirectML.$version"
    Download-FileChecked "https://www.nuget.org/api/v2/package/Microsoft.AI.DirectML/$version" $archive
    Expand-Zip $archive $stage
    Copy-Item -Force (Join-Path $stage 'bin\x64-win\DirectML.dll') (Join-Path $InstallDir 'DirectML.dll')
    Copy-IfExists (Join-Path $stage 'LICENSE.txt') (Join-Path $licenses 'Microsoft.AI.DirectML-LICENSE.txt')
    Copy-IfExists (Join-Path $stage 'LICENSE-CODE.txt') (Join-Path $licenses 'Microsoft.AI.DirectML-LICENSE-CODE.txt')
    Copy-IfExists (Join-Path $stage 'ThirdPartyNotices.txt') (Join-Path $licenses 'Microsoft.AI.DirectML-ThirdPartyNotices.txt')
}

function Has-NvidiaGpu {
    try {
        $out = & nvidia-smi -L 2>$null
        return ($LASTEXITCODE -eq 0 -and ($out -match '^GPU '))
    } catch {
        return $false
    }
}

function Install-SherpaOnnx {
    $version = 'v1.13.0'
    if (Has-NvidiaGpu) {
        $asset = "sherpa-onnx-$version-cuda-12.x-cudnn-9.x-win-x64-cuda.tar.bz2"
    } else {
        $asset = "sherpa-onnx-$version-win-x64-shared-MD-Release-no-tts.tar.bz2"
    }
    $archive = Join-Path $cache $asset
    $stage = Join-Path $cache "sherpa-onnx-$version"
    Download-FileChecked "https://github.com/k2-fsa/sherpa-onnx/releases/download/$version/$asset" $archive
    Reset-Dir $stage
    & tar -xjf $archive -C $stage
    if ($LASTEXITCODE -ne 0) {
        throw "tar failed extracting $archive"
    }
    $offline = Get-ChildItem -Path $stage -Recurse -Filter 'sherpa-onnx-offline.exe' | Select-Object -First 1
    if ($null -eq $offline) {
        throw "sherpa-onnx archive layout unexpected"
    }
    $bin = $offline.Directory.FullName
    Get-ChildItem -Path $bin -Filter '*.dll' | ForEach-Object {
        Copy-Item -Force $_.FullName (Join-Path $InstallDir $_.Name)
    }
    $sherpaLicenses = Join-Path $licenses 'sherpa-onnx'
    New-Item -ItemType Directory -Force -Path $sherpaLicenses | Out-Null
    Get-ChildItem -Path $stage -Recurse -File | Where-Object {
        $_.Name -match '^(LICENSE|NOTICE|ThirdParty|COPYING)'
    } | ForEach-Object {
        Copy-Item -Force $_.FullName (Join-Path $sherpaLicenses $_.Name)
    }
}

Install-SherpaOnnx
Install-OnnxRuntimeDirectML
Install-DirectML
