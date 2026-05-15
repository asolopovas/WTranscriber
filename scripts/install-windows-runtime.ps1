param(
    [Parameter(Mandatory = $true)]
    [string]$InstallDir
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$cache = Join-Path $env:TEMP 'wtranscriber-runtime'
$runtimeLog = Join-Path $env:TEMP 'wtranscriber-runtime-install.log'
$licenses = Join-Path $InstallDir 'licenses'
New-Item -ItemType Directory -Force -Path $cache, $licenses | Out-Null
Remove-Item -Force -ErrorAction SilentlyContinue $runtimeLog

function Write-SetupLog([string]$Message) {
    $line = "$(Get-Date -Format 'HH:mm:ss') $Message"
    [Console]::Out.WriteLine($line)
    Add-Content -Path $runtimeLog -Value $line -Encoding UTF8
}

function Format-ByteSize([long]$Bytes) {
    if ($Bytes -ge 1GB) {
        return "{0:N1} GB" -f ($Bytes / 1GB)
    }
    if ($Bytes -ge 1MB) {
        return "{0:N1} MB" -f ($Bytes / 1MB)
    }
    if ($Bytes -ge 1KB) {
        return "{0:N1} KB" -f ($Bytes / 1KB)
    }
    return "$Bytes bytes"
}

function Download-FileChecked([string]$Url, [string]$OutFile) {
    $name = Split-Path -Leaf $OutFile
    if ((Test-Path $OutFile) -and ((Get-Item $OutFile).Length -gt 0)) {
        $size = Format-ByteSize (Get-Item $OutFile).Length
        Write-SetupLog "Using cached $name ($size)"
        return
    }
    $tmp = "$OutFile.tmp"
    Write-SetupLog "Downloading $name"
    Remove-Item -Force -ErrorAction SilentlyContinue $tmp
    Invoke-WebRequest -Uri $Url -OutFile $tmp -UseBasicParsing
    Move-Item -Force $tmp $OutFile
    $size = Format-ByteSize (Get-Item $OutFile).Length
    Write-SetupLog "Downloaded $name ($size)"
}

function Reset-Dir([string]$Path) {
    Write-SetupLog "Preparing $Path"
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $Path
    New-Item -ItemType Directory -Force -Path $Path | Out-Null
}

function Expand-Zip([string]$Archive, [string]$Destination) {
    Write-SetupLog "Extracting $(Split-Path -Leaf $Archive)"
    Reset-Dir $Destination
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [System.IO.Compression.ZipFile]::ExtractToDirectory($Archive, $Destination)
    Write-SetupLog "Extracted to $Destination"
}

function Copy-IfExists([string]$Source, [string]$Destination) {
    if (Test-Path $Source) {
        Copy-Item -Force $Source $Destination
    }
}

function Install-OnnxRuntimeDirectML {
    $version = '1.24.2'
    Write-SetupLog "Installing ONNX Runtime DirectML $version"
    $archive = Join-Path $cache "Microsoft.ML.OnnxRuntime.DirectML.$version.zip"
    $stage = Join-Path $cache "Microsoft.ML.OnnxRuntime.DirectML.$version"
    Download-FileChecked "https://www.nuget.org/api/v2/package/Microsoft.ML.OnnxRuntime.DirectML/$version" $archive
    Expand-Zip $archive $stage
    $native = Join-Path $stage 'runtimes\win-x64\native'
    Write-SetupLog 'Copying ONNX Runtime DLLs'
    Copy-Item -Force (Join-Path $native 'onnxruntime.dll') (Join-Path $InstallDir 'onnxruntime.dll')
    Copy-Item -Force (Join-Path $native 'onnxruntime_providers_shared.dll') (Join-Path $InstallDir 'onnxruntime_providers_shared.dll')
    Copy-IfExists (Join-Path $stage 'LICENSE') (Join-Path $licenses 'Microsoft.ML.OnnxRuntime.DirectML-LICENSE')
    Copy-IfExists (Join-Path $stage 'ThirdPartyNotices.txt') (Join-Path $licenses 'Microsoft.ML.OnnxRuntime.DirectML-ThirdPartyNotices.txt')
    Write-SetupLog 'ONNX Runtime DirectML installed'
}

function Install-DirectML {
    $version = '1.15.4'
    Write-SetupLog "Installing DirectML $version"
    $archive = Join-Path $cache "Microsoft.AI.DirectML.$version.zip"
    $stage = Join-Path $cache "Microsoft.AI.DirectML.$version"
    Download-FileChecked "https://www.nuget.org/api/v2/package/Microsoft.AI.DirectML/$version" $archive
    Expand-Zip $archive $stage
    Write-SetupLog 'Copying DirectML.dll'
    Copy-Item -Force (Join-Path $stage 'bin\x64-win\DirectML.dll') (Join-Path $InstallDir 'DirectML.dll')
    Copy-IfExists (Join-Path $stage 'LICENSE.txt') (Join-Path $licenses 'Microsoft.AI.DirectML-LICENSE.txt')
    Copy-IfExists (Join-Path $stage 'LICENSE-CODE.txt') (Join-Path $licenses 'Microsoft.AI.DirectML-LICENSE-CODE.txt')
    Copy-IfExists (Join-Path $stage 'ThirdPartyNotices.txt') (Join-Path $licenses 'Microsoft.AI.DirectML-ThirdPartyNotices.txt')
    Write-SetupLog 'DirectML installed'
}

function Get-NvidiaSmiPath {
    $cmd = Get-Command 'nvidia-smi.exe' -ErrorAction SilentlyContinue
    if ($null -ne $cmd) {
        return $cmd.Source
    }
    $candidates = @(
        (Join-Path $env:SystemRoot 'System32\nvidia-smi.exe'),
        (Join-Path $env:SystemRoot 'Sysnative\nvidia-smi.exe'),
        (Join-Path $env:ProgramW6432 'NVIDIA Corporation\NVSMI\nvidia-smi.exe'),
        (Join-Path $env:ProgramFiles 'NVIDIA Corporation\NVSMI\nvidia-smi.exe')
    )
    foreach ($candidate in $candidates) {
        if ($candidate -and (Test-Path $candidate)) {
            return $candidate
        }
    }
    return $null
}

function Has-NvidiaGpu {
    try {
        $nvidiaSmi = Get-NvidiaSmiPath
        if ($null -ne $nvidiaSmi) {
            $out = & $nvidiaSmi -L 2>$null
            if ($LASTEXITCODE -eq 0 -and ($out -match '^GPU ')) {
                return $true
            }
        }
        $controllers = Get-CimInstance -ClassName Win32_VideoController -ErrorAction SilentlyContinue
        return ($null -ne ($controllers | Where-Object { $_.Name -match 'NVIDIA' -or $_.AdapterCompatibility -match 'NVIDIA' } | Select-Object -First 1))
    } catch {
        return $false
    }
}

function Get-NvidiaComputeCapability {
    try {
        $nvidiaSmi = Get-NvidiaSmiPath
        if ($null -eq $nvidiaSmi) {
            return $null
        }
        $rows = & $nvidiaSmi --query-gpu=name,compute_cap --format=csv,noheader,nounits 2>$null
        if ($LASTEXITCODE -ne 0 -or $null -eq $rows) {
            return $null
        }
        foreach ($row in @($rows)) {
            $parts = "$row".Split(',') | ForEach-Object { $_.Trim() }
            if ($parts.Count -ge 2 -and $parts[1] -match '^(\d+)\.(\d+)$') {
                return "$($Matches[1])$($Matches[2])"
            }
        }
    } catch {
        return $null
    }
    return $null
}

function Get-ReleaseSha256([string]$ReleaseTag, [string]$Asset) {
    $sums = Join-Path $cache "SHA256SUMS-$ReleaseTag"
    $url = "https://github.com/asolopovas/WTranscriber/releases/download/$ReleaseTag/SHA256SUMS"
    Download-FileChecked $url $sums
    $line = Get-Content $sums | Where-Object { $_ -match "\s+$([regex]::Escape($Asset))$" } | Select-Object -First 1
    if ($null -eq $line) {
        throw "checksum for $Asset not found in SHA256SUMS"
    }
    return ($line -split '\s+')[0]
}

function Assert-Sha256([string]$Path, [string]$Expected) {
    $actual = (Get-FileHash -Algorithm SHA256 $Path).Hash.ToLowerInvariant()
    if ($actual -ne $Expected.ToLowerInvariant()) {
        Remove-Item -Force -ErrorAction SilentlyContinue $Path
        throw "checksum mismatch for $(Split-Path -Leaf $Path): expected $Expected, got $actual"
    }
}

function Install-WhisperCudaWorker {
    $arch = Get-NvidiaComputeCapability
    if ($null -eq $arch) {
        Write-SetupLog 'No NVIDIA compute capability detected; skipping Whisper CUDA worker'
        return
    }
    $supported = @('61', '75', '80', '86', '89')
    if ($supported -notcontains $arch) {
        Write-SetupLog "NVIDIA compute capability sm_$arch is not packaged; skipping Whisper CUDA worker"
        return
    }
    $releaseTag = if ($env:WT_CUDA_WORKER_TAG) { $env:WT_CUDA_WORKER_TAG } else { 'cuda' }
    $asset = "wtranscriber-cuda-sm$arch-win-x64.zip"
    Write-SetupLog "Installing Whisper CUDA worker sm_$arch from GitHub release $releaseTag"
    $archive = Join-Path $cache $asset
    $expected = Get-ReleaseSha256 $releaseTag $asset
    Download-FileChecked "https://github.com/asolopovas/WTranscriber/releases/download/$releaseTag/$asset" $archive
    Assert-Sha256 $archive $expected
    $stage = Join-Path $cache "wtranscriber-cuda-sm$arch"
    Expand-Zip $archive $stage
    $exe = Get-ChildItem -Path $stage -Recurse -Filter 'wt-whisper-cuda-worker.exe' | Select-Object -First 1
    if ($null -eq $exe) {
        throw "CUDA worker archive layout unexpected"
    }
    $dst = Join-Path $InstallDir 'runtime\cuda'
    New-Item -ItemType Directory -Force -Path $dst | Out-Null
    Copy-Item -Force $exe.FullName (Join-Path $dst 'wt-whisper-cuda-worker.exe')
    Set-Content -Path (Join-Path $dst 'arch.txt') -Value "sm_$arch" -Encoding ASCII
    Write-SetupLog "Whisper CUDA worker sm_$arch installed"
}

function Install-SherpaOnnx {
    $version = 'v1.13.0'
    Write-SetupLog "Installing sherpa-onnx speech runtime $version"
    if (Has-NvidiaGpu) {
        Write-SetupLog 'NVIDIA GPU detected; using the CUDA runtime package'
        $asset = "sherpa-onnx-$version-cuda-12.x-cudnn-9.x-win-x64-cuda.tar.bz2"
    } else {
        Write-SetupLog 'No NVIDIA GPU detected; using the CPU runtime package'
        $asset = "sherpa-onnx-$version-win-x64-shared-MD-Release-no-tts.tar.bz2"
    }
    $archive = Join-Path $cache $asset
    $stage = Join-Path $cache "sherpa-onnx-$version"
    Download-FileChecked "https://github.com/k2-fsa/sherpa-onnx/releases/download/$version/$asset" $archive
    Reset-Dir $stage
    Write-SetupLog "Extracting $asset"
    & tar -xjf $archive -C $stage
    if ($LASTEXITCODE -ne 0) {
        throw "tar failed extracting $archive"
    }
    Write-SetupLog "Extracted to $stage"
    $offline = Get-ChildItem -Path $stage -Recurse -Filter 'sherpa-onnx-offline.exe' | Select-Object -First 1
    if ($null -eq $offline) {
        throw "sherpa-onnx archive layout unexpected"
    }
    $bin = $offline.Directory.FullName
    $dlls = @(Get-ChildItem -Path $bin -Filter '*.dll')
    Write-SetupLog "Copying $($dlls.Count) sherpa-onnx DLLs"
    $dlls | ForEach-Object {
        Copy-Item -Force $_.FullName (Join-Path $InstallDir $_.Name)
    }
    $sherpaLicenses = Join-Path $licenses 'sherpa-onnx'
    New-Item -ItemType Directory -Force -Path $sherpaLicenses | Out-Null
    Get-ChildItem -Path $stage -Recurse -File | Where-Object {
        $_.Name -match '^(LICENSE|NOTICE|ThirdParty|COPYING)'
    } | ForEach-Object {
        Copy-Item -Force $_.FullName (Join-Path $sherpaLicenses $_.Name)
    }
    Write-SetupLog 'sherpa-onnx installed'
}

try {
    Write-SetupLog "Installing runtime dependencies into $InstallDir"
    Write-SetupLog "Download cache: $cache"
    Write-SetupLog "Install log: $runtimeLog"
    Install-SherpaOnnx
    Install-OnnxRuntimeDirectML
    Install-DirectML
    try {
        Install-WhisperCudaWorker
    } catch {
        Write-SetupLog "Optional Whisper CUDA worker installation skipped: $($_.Exception.Message)"
    }
    Write-SetupLog 'Runtime dependency installation complete'
} catch {
    Write-SetupLog "Runtime dependency installation failed: $($_.Exception.Message)"
    throw
}
