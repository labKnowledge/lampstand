#!/usr/bin/env bash
# Builds Lampstand for Linux in an Ubuntu 22.04 container and packages it as
# a .deb and an .rpm in dist/.
#
#   linux/package-linux.sh
#
# Both packages pull in FFmpeg through the package manager, so streaming,
# recording, videos and cameras work right after install.
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
IMAGE=lampstand-linux-build:22.04
OUT=dist
mkdir -p "$OUT" target/linux-compat

docker build -q -t "$IMAGE" -f linux/Dockerfile.build linux >/dev/null

# Runs as you, so nothing in the project ends up owned by root.
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$PWD:/work" -w /work \
  -e CARGO_HOME=/work/target/linux-compat/cargo-home \
  -e CARGO_TARGET_DIR=/work/target/linux-compat \
  -e HOME=/tmp \
  -e VERSION="$VERSION" \
  "$IMAGE" bash -euo pipefail -c '
    cargo build --release --locked
    BIN=target/linux-compat/release/lampstand
    STAGE=$(mktemp -d) && chmod 755 "$STAGE"
    install -Dm755 "$BIN" "$STAGE/usr/bin/lampstand"
    install -Dm644 linux/lampstand.desktop "$STAGE/usr/share/applications/lampstand.desktop"
    for s in 16 24 32 48 64 128 256 512; do
      install -Dm644 "linux/icons/$s.png" "$STAGE/usr/share/icons/hicolor/${s}x${s}/apps/lampstand.png"
    done
    install -Dm644 assets/fonts/OFL-Figtree.txt "$STAGE/usr/share/doc/lampstand/OFL-Figtree.txt"
    install -Dm644 assets/fonts/OFL-PlayfairDisplay.txt "$STAGE/usr/share/doc/lampstand/OFL-PlayfairDisplay.txt"
    SIZE_KB=$(du -sk "$STAGE/usr" | cut -f1)

    # ---- .deb
    DEB=$(mktemp -d) && chmod 755 "$DEB"
    cp -a "$STAGE/." "$DEB/"
    mkdir -p "$DEB/DEBIAN"
    cat > "$DEB/DEBIAN/control" <<EOF
Package: lampstand
Version: $VERSION
Architecture: amd64
Maintainer: Eligapris <support@eligapris.com>
Installed-Size: $SIZE_KB
Depends: libasound2 | libasound2t64, libgl1, libegl1, libxkbcommon0, libxkbcommon-x11-0, libx11-6, libxcursor1, libxrandr2, libxi6, ffmpeg
Recommends: xdg-desktop-portal
Section: video
Priority: optional
Homepage: https://eligapris.com/lampstand
Description: Church presentation and live stream
 Lampstand puts songs, Bible verses, announcements and on-air graphics on the
 church screen and the live stream from one window. It records and streams
 with FFmpeg and works with cameras, capture cards and screen sharing.
EOF
    cat > "$DEB/DEBIAN/postinst" <<"EOF"
#!/bin/sh
set -e
command -v update-desktop-database >/dev/null && update-desktop-database -q /usr/share/applications || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t /usr/share/icons/hicolor || true
EOF
    chmod 755 "$DEB/DEBIAN/postinst"
    dpkg-deb --root-owner-group --build "$DEB" "dist/lampstand_${VERSION}_amd64.deb" >/dev/null

    # ---- .rpm
    TOP=$(mktemp -d)
    mkdir -p "$TOP/SPECS"
    cat > "$TOP/SPECS/lampstand.spec" <<EOF
Name: lampstand
Version: $VERSION
Release: 1
Summary: Church presentation and live stream
License: Proprietary
URL: https://eligapris.com/lampstand
Requires: alsa-lib, mesa-libGL, mesa-libEGL, libxkbcommon, libxkbcommon-x11, libX11, libXcursor, libXrandr, libXi, /usr/bin/ffmpeg
Recommends: xdg-desktop-portal
AutoReqProv: no

%description
Lampstand puts songs, Bible verses, announcements and on-air graphics on the
church screen and the live stream from one window. It records and streams
with FFmpeg and works with cameras, capture cards and screen sharing.

%install
cp -a $STAGE/. %{buildroot}/

%files
/usr/bin/lampstand
/usr/share/applications/lampstand.desktop
/usr/share/icons/hicolor/*/apps/lampstand.png
/usr/share/doc/lampstand

%post
update-desktop-database -q /usr/share/applications >/dev/null 2>&1 || :
gtk-update-icon-cache -q -t /usr/share/icons/hicolor >/dev/null 2>&1 || :
EOF
    rpmbuild -bb --quiet --define "_topdir $TOP" --define "_rpmdir $TOP/RPMS" \
      --define "_build_id_links none" --target x86_64 "$TOP/SPECS/lampstand.spec" >/dev/null
    cp "$TOP"/RPMS/x86_64/lampstand-"$VERSION"-1.x86_64.rpm dist/
  '

ls -la "$OUT/lampstand_${VERSION}_amd64.deb" "$OUT/lampstand-${VERSION}-1.x86_64.rpm"
