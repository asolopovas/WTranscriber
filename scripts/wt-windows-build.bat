@echo off
setlocal EnableExtensions
set SHA=%1
set REPO=%~2
set ARCHIVE=%~3
if "%SHA%"=="" ( echo Usage: wt-windows-build.bat ^<sha^> [repo-dir] [source-archive] 1^>^&2 & exit /b 2 )
if "%REPO%"=="" set "REPO=C:\WTranscriber"

set "VCVARS=C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if not exist "%VCVARS%" (
  echo [win] MSVC build tools missing. Install via: 1>&2
  echo [win]   winget install --id Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended" 1>&2
  exit /b 90
)
call "%VCVARS%" >NUL || exit /b %ERRORLEVEL%

set "PATH=%USERPROFILE%\.cargo\bin;%USERPROFILE%\.bun\bin;C:\Program Files\just;C:\Program Files\nodejs;%PATH%"

where tar >NUL 2>&1 || ( echo [win] tar missing on PATH 1>&2 & exit /b 91 )
where just >NUL 2>&1 || ( echo [win] just missing on PATH; run scripts\bootstrap-windows.ps1 1>&2 & exit /b 92 )
where bun  >NUL 2>&1 || ( echo [win] bun missing on PATH 1>&2 & exit /b 93 )
where cargo >NUL 2>&1 || ( echo [win] cargo missing on PATH 1>&2 & exit /b 94 )

if "%ARCHIVE%"=="" ( echo [win] source archive argument missing 1>&2 & exit /b 96 )
if not exist "%ARCHIVE%" ( echo [win] source archive not found: %ARCHIVE% 1>&2 & exit /b 97 )
if exist "%REPO%" ( rmdir /S /Q "%REPO%" 2>NUL )
mkdir "%REPO%" || exit /b %ERRORLEVEL%
tar -xzf "%ARCHIVE%" -C "%REPO%" || exit /b %ERRORLEVEL%
cd /D "%REPO%" || exit /b %ERRORLEVEL%

rem --- self-heal rustup target if corrupt --------------------------------
rustup target add x86_64-pc-windows-msvc 2>nul 1>nul
if errorlevel 1 (
  echo [win] rustup target add reported error; attempting repair
  rustup component remove --target x86_64-pc-windows-msvc rust-std 2>nul 1>nul
  rustup self update 2>nul 1>nul
  rustup target add x86_64-pc-windows-msvc
  if errorlevel 1 (
    echo [win] reinstalling toolchain 1>&2
    for /f %%T in ('rustup show active-toolchain 2^>nul') do set ACTIVE=%%T
    rustup toolchain uninstall %ACTIVE% 2>nul
    rustup toolchain install %ACTIVE% --profile minimal --component rust-std --target x86_64-pc-windows-msvc || exit /b 95
  )
)

call bun install --frozen-lockfile --no-progress || exit /b %ERRORLEVEL%
call just build || exit /b %ERRORLEVEL%

dir src-tauri\target\release\bundle\nsis\*-setup.exe
