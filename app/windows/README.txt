Building Lampstand on a Windows PC over your network

1. On the Windows PC, open File Explorer and go to the shared folder that holds this "app" folder
   (for example \\CHURCH-MAC\Lampstand\app). Copying the folder to the PC works too.
2. Open the "windows" folder and double-click "Build Lampstand for Windows.bat".
3. If Rust is missing, answer Y and let it install, then double-click the file again.
4. The first build takes 5 to 15 minutes. When it finishes, a folder called "dist-windows"
   opens with Lampstand.exe in it. Copy that folder to any Windows PC that should run Lampstand.

What the build does
- It keeps its working files on the Windows PC (in %LOCALAPPDATA%\LampstandBuild), so the shared
  folder stays small and the build stays fast.
- It copies FFmpeg next to Lampstand.exe when the PC already has it, so streaming, recording,
  videos, music and cameras work straight away. If FFmpeg is missing it offers to install it.

Good to know on Windows
- Allow camera access for desktop apps in Settings, Privacy & security, Camera.
- Windows may show a SmartScreen warning the first time, because the program isn't signed.
  Choose "More info", then "Run anyway".
- Build again after any change to Lampstand to get the newest version.
