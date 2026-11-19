# Lampstand

Church presentation and live stream in one desktop app. Lampstand puts songs,
Bible verses and announcements on the church screen and streams the service to
YouTube, Facebook, Twitch or Vimeo with name bars, captions and a ticker. It
runs on Windows, macOS and Linux.

Download: https://eligapris.com/lampstand

## Layout

The app lives in `app/` (Rust, eframe/egui).

| Path | What it holds |
|---|---|
| `app/src/` | The app. `engine.rs` and `render.rs` draw the screen and stream; `ui_*.rs` are the panels |
| `app/assets/` | Bibles (WEB, KJV) and fonts, compiled into the binary; the window icon |
| `app/linux/` | `.deb`/`.rpm` packaging, built in an Ubuntu 22.04 container |
| `app/windows/` | The `.msi` (WiX 3) and the older build-on-the-PC batch file |
| `app/macos/` | The universal `.app` and `.dmg` |
| `app/scripts/` | `release.sh` (build everything and publish) and `build-remote.sh` |
| `app/shots/` | Screenshots the app renders of itself (`--screenshot`) |

## Run it

```bash
cd app
cargo run --release
```

FFmpeg is needed for streaming, recording, video and cameras. Lampstand finds it
next to itself, on `PATH`, or in the usual install folders, and the status bar
says whether it was found.

## Release

1. Bump `version` in `app/Cargo.toml` (normal release `+0.0.1`, big one `+0.0.5`).
2. Run:

```bash
cd app
scripts/release.sh --notes "Two or three plain lines on what changed"
```

It builds the Linux `.deb` and `.rpm` here (Docker), the Windows `.msi` on the
build PC and the macOS `.dmg` on the build Mac (both over SSH, the same machines
Samepad uses), then uploads each one to eligapris.com. A machine that's asleep is
skipped and reported; the rest still ship.

The upload needs an eligapris.com API key with the `releases:write` scope, in
`ELIGAPRIS_RELEASE_KEY` or `~/.config/eligapris/release-key`. The stable download
links are `https://eligapris.com/download/lampstand` (picks the visitor's system)
and `/download/lampstand/{windows,mac,linux-deb,linux-rpm}`.
