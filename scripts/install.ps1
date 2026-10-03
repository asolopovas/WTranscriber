$ErrorActionPreference = 'Stop'
$option = $env:option
if ($option -eq '--help' -or $option -eq '-h') {
    Write-Host 'just install: bootstrap, build and install the current checkout on Windows or Linux.'
    Write-Host 'Use --interactive for the Windows installer UI. No git pull or branch switch is performed.'
    exit 0
}
if ($option -and $option -ne '--interactive') {
    throw "Unknown option: $option (use just install --help)"
}
Set-Location -LiteralPath (Join-Path $PSScriptRoot '..')
if (Test-Path -LiteralPath 'tmp/_pids.json') {
    throw 'Stop the live dev session with just stop before building.'
}

$stamp = 'tmp/.setup.stamp'
$bootstrap = Join-Path $PSScriptRoot 'bootstrap-windows.ps1'
$needsSetup = -not (Get-Command bun -ErrorAction SilentlyContinue) -or
    -not (Get-Command cargo -ErrorAction SilentlyContinue) -or
    -not (Test-Path -LiteralPath $stamp)
if (-not $needsSetup) {
    $needsSetup = (Get-Item -LiteralPath $bootstrap).LastWriteTimeUtc -gt (Get-Item -LiteralPath $stamp).LastWriteTimeUtc
}
if ($needsSetup) {
    & powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File $bootstrap
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:USERPROFILE\.bun\bin;" +
        [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
        [Environment]::GetEnvironmentVariable('Path', 'User') + ';' + $env:Path
    foreach ($name in @('LIBCLANG_PATH', 'CUDA_PATH', 'RUSTC_WRAPPER', 'CMAKE_C_COMPILER_LAUNCHER', 'CMAKE_CXX_COMPILER_LAUNCHER', 'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER')) {
        $value = [Environment]::GetEnvironmentVariable($name, 'User')
        if ($value) { Set-Item -LiteralPath "env:$name" -Value $value }
    }
}
& bun install
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
New-Item -ItemType Directory -Force -Path 'tmp' | Out-Null
Set-Content -LiteralPath $stamp -Value ([DateTime]::UtcNow.ToString('o'))
$env:CARGO_TARGET_DIR = Join-Path (Get-Location).Path 'src-tauri\target'
$env:CARGO_INCREMENTAL = '0'
Remove-Item -LiteralPath 'env:CARGO_BUILD_TARGET' -ErrorAction SilentlyContinue
& bun scripts/run.ts --tag install-build --idle 1800 --max 7200 -- bun run tauri build --bundles nsis -- --locked --no-default-features --features directml
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$installArgs = @('scripts/install-dev.ts')
if ($option) { $installArgs += $option }
& bun @installArgs
exit $LASTEXITCODE
