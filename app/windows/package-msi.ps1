# Builds Lampstand for Windows and wraps it in an .msi (dist\Lampstand-<version>-x64.msi).
# Runs on the Windows build PC; scripts/build-remote.sh windows calls it over SSH.
# Needs Rust (MSVC), the Visual Studio C++ build tools and WiX Toolset 3.14.
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)
$Root = (Get-Location).Path
$Version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$env:Path = "$env:USERPROFILE\.cargo\bin;C:\Program Files (x86)\WiX Toolset v3.14\bin;$env:Path"
$env:CARGO_TARGET_DIR = "$env:LOCALAPPDATA\LampstandBuild\target"

if (-not (Test-Path third_party\ffmpeg\windows\ffmpeg.exe)) { throw "third_party\ffmpeg\windows\ffmpeg.exe is missing: run scripts/fetch-ffmpeg.sh first" }
Write-Host "Building Lampstand $Version for Windows..."
cargo build --release --locked
if ($LASTEXITCODE) { exit $LASTEXITCODE }

$Exe = "$env:CARGO_TARGET_DIR\release\lampstand.exe"
$Obj = "$env:LOCALAPPDATA\LampstandBuild\wix"
New-Item -ItemType Directory -Force -Path $Obj, dist | Out-Null
$Msi = "dist\Lampstand-$Version-x64.msi"

candle.exe -nologo -arch x64 -ext WixUtilExtension "-dVersion=$Version" "-dRoot=$Root" "-dExe=$Exe" `
  -out "$Obj\lampstand.wixobj" windows\lampstand.wxs
if ($LASTEXITCODE) { exit $LASTEXITCODE }
# ICE38/43/57/61/91 assume shortcuts go to one user's profile. This install is per machine
# (ALLUSERS=1), so the Start menu and desktop folders are the all-users ones and these don't apply.
light.exe -nologo -ext WixUtilExtension -sice:ICE38 -sice:ICE43 -sice:ICE57 -sice:ICE61 -sice:ICE91 -spdb -out $Msi "$Obj\lampstand.wixobj"
if ($LASTEXITCODE) { exit $LASTEXITCODE }
Get-Item $Msi | Select-Object Name, Length
