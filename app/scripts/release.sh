#!/usr/bin/env bash
# Builds Lampstand for every desktop it can reach and publishes it to
# eligapris.com/lampstand (download links: eligapris.com/download/lampstand/<platform>).
#
#   scripts/release.sh --notes "What changed, in two or three plain lines"
#
#   - Linux .deb and .rpm: here, in an Ubuntu 22.04 container (linux/package-linux.sh)
#   - Windows .msi: on the Windows PC on the network (scripts/build-remote.sh windows)
#   - macOS .dmg (Apple chips + Intel in one app): on the Mac (scripts/build-remote.sh mac)
# A machine that can't be reached is skipped and reported; the others still ship.
#
# Version: bump `version` in Cargo.toml (0.0.1 → 0.0.2 for a normal release,
# +0.0.5 for a big one). Everything else follows it.
#
# Env:
#   ELIGAPRIS_RELEASE_KEY  an eligapris.com API key with the releases:write scope.
#                          Default: read from ~/.config/eligapris/release-key
#   ELIGAPRIS_URL          default https://eligapris.com
#   --skip-build           publish what's already in dist/
set -euo pipefail
cd "$(dirname "$0")/.."

NOTES=""
SKIP_BUILD=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --notes) NOTES="$2"; shift 2 ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
done

URL="${ELIGAPRIS_URL:-https://eligapris.com}"
KEY_FILE="$HOME/.config/eligapris/release-key"
KEY="${ELIGAPRIS_RELEASE_KEY:-$( [[ -r $KEY_FILE ]] && cat "$KEY_FILE" || true )}"
[[ -n "$KEY" ]] || { echo "No release key: set ELIGAPRIS_RELEASE_KEY or write it to $KEY_FILE" >&2; exit 1; }
VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
echo "Lampstand $VERSION -> $URL"

# After a version bump Cargo.lock still has the old number; the --locked builds need it current.
cargo update --workspace --offline --quiet
# The Mac binary carries its own copy of the version; keep it in step with Cargo.toml.
sed -i -E '/CFBundleShortVersionString/{n;s#<string>[^<]*</string>#<string>'"$VERSION"'</string>#}' macos/Info.plist

declare -A RESULT
if [[ $SKIP_BUILD -eq 0 ]]; then
  scripts/fetch-ffmpeg.sh >/dev/null   # bundled into the Windows and Mac installers
  if linux/package-linux.sh; then RESULT[linux]=built; else RESULT[linux]="build failed"; fi
  reachable() { ssh -o BatchMode=yes -o ConnectTimeout=6 "$1" 'exit 0' >/dev/null 2>&1; }
  for target in windows mac; do
    var="LAMPSTAND_${target^^}_SSH"; host="${!var:-samepad-$target}"
    if ! reachable "$host"; then RESULT[$target]="skipped: $host not reachable"
    elif scripts/build-remote.sh "$target"; then RESULT[$target]=built
    else RESULT[$target]="build failed"; fi
  done
fi

publish() { # platform file
  local platform="$1" file="$2"
  if [[ ! -f "$file" ]]; then printf '  %-10s no build for %s here\n' "$platform" "$VERSION"; return; fi
  printf '  %-10s %-6s ' "$platform" "$(du -h "$file" | cut -f1)"
  if curl -fsS -X POST -T "$file" -H "Authorization: Bearer $KEY" -H "Content-Type: application/octet-stream" \
       --url-query "platform=$platform" --url-query "version=$VERSION" --url-query "notes=$NOTES" \
       "$URL/api/releases/lampstand" >/dev/null; then
    echo "published"
  else
    echo "upload failed"
  fi
}

echo "Publishing:"
publish windows "dist/Lampstand-$VERSION-x64.msi"
publish mac "dist/Lampstand-$VERSION.dmg"
publish linux-deb "dist/lampstand_${VERSION}_amd64.deb"
publish linux-rpm "dist/lampstand-$VERSION-1.x86_64.rpm"

if [[ $SKIP_BUILD -eq 0 ]]; then
  echo "Builds:"
  for k in linux windows mac; do printf '  %-8s %s\n' "$k" "${RESULT[$k]:-not run}"; done
fi
echo "Page: $URL/lampstand"
