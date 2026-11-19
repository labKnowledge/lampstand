@echo off
setlocal EnableExtensions
title Lampstand: build for Windows
pushd "%~dp0.."
if errorlevel 1 (
  echo Could not open the project folder. Copy the whole app folder to this PC and try again.
  pause
  exit /b 1
)
set "PROJECT=%CD%"
set "OUT=%PROJECT%\dist-windows"
set "CARGO_TARGET_DIR=%LOCALAPPDATA%\LampstandBuild\target"
echo.
echo  Lampstand: building the Windows version
echo  Project folder: %PROJECT%
echo  Build files go to: %CARGO_TARGET_DIR%
echo.

where cargo >nul 2>nul
if errorlevel 1 goto :no_rust

echo  Step 1 of 3: building (the first time takes 5 to 15 minutes)...
cargo build --release
if errorlevel 1 goto :build_failed

echo.
echo  Step 2 of 3: copying the program...
if not exist "%OUT%" mkdir "%OUT%"
copy /Y "%CARGO_TARGET_DIR%\release\lampstand.exe" "%OUT%\Lampstand.exe" >nul
if errorlevel 1 goto :copy_failed

echo  Step 3 of 3: looking for FFmpeg (needed for streaming, recording, video and cameras)...
set "FFMPEG="
for /f "delims=" %%F in ('where ffmpeg 2^>nul') do if not defined FFMPEG set "FFMPEG=%%F"
if defined FFMPEG (
  copy /Y "%FFMPEG%" "%OUT%\ffmpeg.exe" >nul
  echo  FFmpeg copied next to Lampstand.exe
) else (
  echo  FFmpeg was not found on this PC.
  where winget >nul 2>nul
  if not errorlevel 1 (
    choice /C YN /M "  Install FFmpeg now with winget"
    if not errorlevel 2 winget install --id Gyan.FFmpeg -e --accept-package-agreements --accept-source-agreements
  )
  echo  After FFmpeg is installed, Lampstand finds it by itself.
)

echo.
echo  Done. Lampstand is ready in:
echo  %OUT%\Lampstand.exe
echo.
start "" "%OUT%"
popd
pause
exit /b 0

:no_rust
echo  Rust (the tool that builds Lampstand) is not installed on this PC.
where winget >nul 2>nul
if errorlevel 1 (
  echo  Download and run rustup-init.exe from https://rustup.rs, then double-click this file again.
  popd
  pause
  exit /b 1
)
choice /C YN /M "  Install Rust and the Microsoft C++ build tools now with winget"
if errorlevel 2 (
  popd
  exit /b 1
)
winget install --id Rustlang.Rustup -e --accept-package-agreements --accept-source-agreements
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --accept-package-agreements --accept-source-agreements --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
echo.
echo  Installed. Close this window, then double-click this file again to build.
popd
pause
exit /b 0

:build_failed
echo.
echo  The build stopped with an error (shown above).
echo  If the message mentions link.exe or "linker", install the Microsoft C++ build tools:
echo    winget install --id Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
echo  then double-click this file again.
popd
pause
exit /b 1

:copy_failed
echo.
echo  The program was built but could not be copied into %OUT%.
echo  Close Lampstand if it is running from that folder, then try again.
popd
pause
exit /b 1
