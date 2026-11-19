#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
APP="dist-mac/Lampstand.app"
WORK="dist-mac/work"
STAGE="dist-mac/work/stage"
ICONSET="dist-mac/work/AppIcon.iconset"
echo "Building Lampstand $VERSION for Apple chips and Intel Macs..."
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$ICONSET" "$STAGE"
lipo -create -output "$APP/Contents/MacOS/Lampstand" target/aarch64-apple-darwin/release/lampstand target/x86_64-apple-darwin/release/lampstand
sed "s/__VERSION__/$VERSION/g" macos/bundle-Info.plist > "$APP/Contents/Info.plist"
printf 'APPL????' > "$APP/Contents/PkgInfo"
for s in 16 32 128 256 512; do
  sips -z "$s" "$s" macos/icon_1024.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2))
  sips -z "$d" "$d" macos/icon_1024.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"
# FFmpeg inside the app (Apple chips + Intel in one file), so streaming, recording,
# videos and cameras work with nothing else to install. Lampstand finds it in Resources.
FF=third_party/ffmpeg/mac
[[ -x "$FF/ffmpeg-arm64" && -x "$FF/ffmpeg-x86_64" ]] || { echo "FFmpeg is missing: run scripts/fetch-ffmpeg.sh first" >&2; exit 1; }
rm -f "$APP/Contents/Resources/ffmpeg"
lipo -create -output "$APP/Contents/Resources/ffmpeg" "$FF/ffmpeg-arm64" "$FF/ffmpeg-x86_64"
chmod 755 "$APP/Contents/Resources/ffmpeg"
cp "$FF/FFMPEG-LICENSE.txt" "$APP/Contents/Resources/FFMPEG-LICENSE.txt"
codesign --force --sign - --identifier app.lampstand.ffmpeg "$APP/Contents/Resources/ffmpeg"
codesign --force --sign - --identifier app.lampstand.live --requirements '=designated => identifier "app.lampstand.live"' "$APP/Contents/MacOS/Lampstand"
codesign --force --sign - --identifier app.lampstand.live --requirements '=designated => identifier "app.lampstand.live"' "$APP"
ditto "$APP" "$STAGE/Lampstand.app"
ln -sfn /Applications "$STAGE/Applications"
cp macos/Install-README.txt "$STAGE/Read me first.txt"
hdiutil create -volname "Lampstand" -srcfolder "$STAGE" -ov -format UDZO "dist-mac/Lampstand-$VERSION.dmg" >/dev/null
echo "Done:"
echo "  $PWD/dist-mac/Lampstand-$VERSION.dmg"
echo "  $PWD/$APP"
