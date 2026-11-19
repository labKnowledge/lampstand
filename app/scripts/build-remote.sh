#!/usr/bin/env bash
# Builds a Lampstand installer on another machine over SSH and copies it into dist/:
#
#   scripts/build-remote.sh windows   # the Windows PC on the network: .msi
#   scripts/build-remote.sh mac       # the Mac on the network: universal .dmg (Apple chips + Intel)
#
# Env:
#   LAMPSTAND_WINDOWS_SSH  default "samepad-windows" (~/.ssh/config host; shared build PC)
#   LAMPSTAND_MAC_SSH      default "samepad-mac"     (~/.ssh/config host; shared build Mac)
set -euo pipefail
cd "$(dirname "$0")/.."
TARGET="${1:?windows or mac}"
VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
case "$TARGET" in
  windows) HOST="${LAMPSTAND_WINDOWS_SSH:-samepad-windows}" ;;
  mac) HOST="${LAMPSTAND_MAC_SSH:-samepad-mac}" ;;
  *) echo "unknown target $TARGET" >&2; exit 2 ;;
esac
SSH_OPTS=(-o BatchMode=yes -o ServerAliveInterval=30)
remote() { ssh "${SSH_OPTS[@]}" "$HOST" "$@"; }

# The sources as they are now (committed or not), without build output.
SRC="$(mktemp --suffix=.tgz)"
trap 'rm -f "$SRC"' EXIT
tar -czf "$SRC" --exclude=./target --exclude=./dist --exclude=./dist-mac --exclude=./dist-windows \
  --exclude=./shots --exclude=.DS_Store .
echo "Sending sources to $TARGET ($HOST, $(du -h "$SRC" | cut -f1))..."
mkdir -p dist

if [[ $TARGET == windows ]]; then
  remote 'if (Test-Path C:\lampstand-src) { Remove-Item -Recurse -Force C:\lampstand-src }; New-Item -ItemType Directory C:\lampstand-src | Out-Null'
  scp -q "${SSH_OPTS[@]}" "$SRC" "$HOST:C:/lampstand-src/src.tgz"
  remote 'cd C:\lampstand-src; tar -xzf src.tgz; powershell -NoProfile -ExecutionPolicy Bypass -File windows\package-msi.ps1; exit $LASTEXITCODE'
  scp -q "${SSH_OPTS[@]}" "$HOST:C:/lampstand-src/dist/Lampstand-$VERSION-x64.msi" dist/
  ls -la "dist/Lampstand-$VERSION-x64.msi"
else
  remote 'rm -rf ~/lampstand-src && mkdir -p ~/lampstand-src'
  scp -q "${SSH_OPTS[@]}" "$SRC" "$HOST:lampstand-src/src.tgz"
  remote 'export PATH="$HOME/.cargo/bin:$PATH"; cd ~/lampstand-src && tar -xzf src.tgz && bash macos/make-mac-installer.sh'
  scp -q "${SSH_OPTS[@]}" "$HOST:lampstand-src/dist-mac/Lampstand-$VERSION.dmg" dist/
  ls -la "dist/Lampstand-$VERSION.dmg"
fi
