#!/usr/bin/env bash
# Downloads the FFmpeg builds that ship inside the Windows and Mac installers,
# checks each against its pinned SHA-256, and keeps them in third_party/ffmpeg/
# (not in git). Already-downloaded files are reused.
#
#   scripts/fetch-ffmpeg.sh
#
# Bundling FFmpeg means streaming, recording, videos and cameras work right
# after install, with nothing else to set up. Linux gets FFmpeg from the
# package manager instead (the .deb and .rpm depend on it).
#
# FFmpeg 9.0.2, GPL builds (they include libx264, Lampstand's software encoder):
#   Windows: Gyan "essentials" build      https://www.gyan.dev/ffmpeg/builds/
#   macOS:   Martin Riedl static builds   https://ffmpeg.martin-riedl.de/
# Shipped next to Lampstand as a separate program, with FFMPEG-LICENSE.txt
# saying where its source is.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=third_party/ffmpeg
CACHE="$OUT/.downloads"
mkdir -p "$CACHE" "$OUT/windows" "$OUT/mac"

fetch() { # url sha256 file
  if [[ ! -f "$3" ]] || ! echo "$2  $3" | sha256sum -c --status; then
    echo "Downloading $(basename "$1")..."
    curl -fsSL --retry 3 -o "$3.part" "$1"
    echo "$2  $3.part" | sha256sum -c --status || { echo "Checksum mismatch for $1" >&2; rm -f "$3.part"; exit 1; }
    mv "$3.part" "$3"
  fi
}

fetch https://github.com/GyanD/codexffmpeg/releases/download/9.0.2/ffmpeg-9.0.2-essentials_build.zip \
  60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba "$CACHE/windows.zip"
fetch https://ffmpeg.martin-riedl.de/download/macos/arm64/1789931890_9.0.2/ffmpeg.zip \
  c8ed4c4e6978a03c485edbfe4e0a5dc2380f8a30bba5150531b31b094492d924 "$CACHE/mac-arm64.zip"
fetch https://ffmpeg.martin-riedl.de/download/macos/amd64/1789931006_9.0.2/ffmpeg.zip \
  7c6b4125b191cbf773832dc51f424cf2b6bb7da43007d1e066f95909e47cacd4 "$CACHE/mac-x86_64.zip"

unzip -o -q -j "$CACHE/windows.zip" '*/bin/ffmpeg.exe' -d "$OUT/windows"
unzip -o -q -p "$CACHE/mac-arm64.zip" ffmpeg > "$OUT/mac/ffmpeg-arm64"
unzip -o -q -p "$CACHE/mac-x86_64.zip" ffmpeg > "$OUT/mac/ffmpeg-x86_64"
chmod +x "$OUT/mac/ffmpeg-arm64" "$OUT/mac/ffmpeg-x86_64"

# The license and source notice that travels with the binary.
{
  cat <<'EOF'
FFmpeg 9.0.2

Lampstand includes FFmpeg, a separate program, to stream, record, play videos
and read cameras. FFmpeg is free software, licensed under the GNU General
Public License version 3 (the full text follows). Lampstand runs it as its own
process and is not part of it.

FFmpeg source code: https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz
Windows build: https://www.gyan.dev/ffmpeg/builds/ (essentials build 9.0.2)
macOS build: https://ffmpeg.martin-riedl.de/ (release 9.0.2), build scripts at
https://git.martin-riedl.de/ffmpeg/build-script
To get a copy of the exact source used, email support@eligapris.com.

EOF
  unzip -p "$CACHE/windows.zip" '*/LICENSE'
} > "$OUT/FFMPEG-LICENSE.txt"
cp "$OUT/FFMPEG-LICENSE.txt" "$OUT/windows/" && cp "$OUT/FFMPEG-LICENSE.txt" "$OUT/mac/"

ls -la "$OUT/windows" "$OUT/mac"
