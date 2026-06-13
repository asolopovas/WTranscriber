# Dev loop

## Task contract

Most `just` recipes wrap `scripts/run.ts`:

- Output is line-prefixed with `[tag]`; per-tag log at `logs/<tag>.log`.
- Idle timeout (`--idle`, default 90 s): kills with `FAIL IDLE_TIMEOUT`, exit 124.
- Hard timeout (`--max`, default 600 s): kills with `FAIL MAX_TIMEOUT`, exit 124.
- Final line: `OK in X.Ys` / `FAIL exit=N in X.Ys`.

`just dev` runs `--idle 0 --max 0` (no watchdog). `just android` runs xtask directly, so the bootstrap is not killed during quiet cargo/Gradle phases.

Verification gates and the change-type matrix live in [`verification.md`](verification.md).

## Desktop

Windows is the primary release host. Linux supports `just dev` and the Docker `.deb` path in `cargo xtask release`. `just build` runs only on a Windows host but builds the full matrix (Windows + Linux `.deb` + Android APK). `bundle.targets = ["nsis", "deb"]`; no macOS `.app`.

```bash
just dev          # HMR (Vite + tauri dev)
just dev stop     # stop any dev session (desktop + android)
just build        # full dev release matrix (Windows host)
just check        # parallel pre-release gate
just check-changed --staged  # changed-file gate (hooks/CI)
```

## Android

```bash
just android                       # clean-start USB HMR session
just android host                  # bootstrap Wi-Fi/LAN session
just android usb <serial>          # pick a device when several are attached
just dev stop                      # stop session and forwards

bun scripts/android-install.ts          # APK-only build + install
bun scripts/android-install.ts --force  # uninstall + reinstall (signature mismatch)
bun scripts/android-emu.ts              # headless x86_64 emulator
```

`just android` is not a no-op: it stops any existing session and force-stops the app before restarting.

## Live-session signals

- HMR proof after JS/CSS edit: `[vite] hmr update /src/...` in `tmp/android-dev.log`.
- USB mode sets `TAURI_DEV_HOST=127.0.0.1` and `adb reverse tcp:1420`/`tcp:1421` so the device reaches Vite over USB. Host mode detects the LAN IP, sets `TAURI_DEV_HOST` to it, and passes `--host`.
- Crash/OOM proof: `am_kill` / `am_proc_died` / `am_crash` for the app in `tmp/logcat.log`.
- `location.href` is not a health signal on Android.

Full `tmp/` artefact contract: [`tmp.md`](tmp.md).

## HMR rule

`src/**` edits hot-reload. Any backend / native / config / capability edit:

```bash
just dev stop && just android
```

While `tmp/_pids.json` exists and Vite owns `:1420`, do not run `cargo xtask android build`, `bun scripts/android-install.ts`, `cargo tauri build`, or any release build — each replaces the debug-dev APK and strands HMR.
