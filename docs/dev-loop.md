# Dev loop

## Task contract

Most `just` recipes run through `scripts/run.ts`:

- Output line-prefixed with `[tag]`.
- Heartbeat after 10 s of silence: `… still running, Xs elapsed, Ys without output`.
- Idle timeout (`--idle`, default 90 s): kills with `FAIL IDLE_TIMEOUT`, exit 124.
- Hard timeout (`--max`, default 600 s): kills with `FAIL MAX_TIMEOUT`, exit 124.
- Final summary: `OK in X.Ys` / `FAIL exit=N in X.Ys`.

`just dev` uses `--idle 0 --max 0`. `just android` runs xtask directly so Android bootstrap is not killed during quiet cargo/Gradle phases. Anything quiet >30 s during steady state is a bug.

`just check` runs `cargo xtask check`, which fans out **11 gates** in parallel: `fmt-check`, `clippy`, `clippy-xtask`, `typecheck`, `vue-lint`, `knip`, `rust-test`, `xtask-test`, `js-test`, `machete`, `audit`. All gates complete before the first failure is reported. Pass job tags for focused runs, e.g. `just check typecheck js-test`.

CI runs `just check-changed --base …` on every push and PR so only checks selected by changed files run there. Use full `just check` locally before releases or wider refactors.

## Desktop

Windows is the primary release host. Linux is supported for desktop dev (`just dev`) and for the Docker-based `.deb` path inside `cargo xtask release`. The `just build` shortcut is Windows-only. `bundle.targets = ["nsis", "deb"]` — macOS `.app` is not configured.

```bash
just dev          # HMR (Vite + tauri dev)
just dev stop     # stop any running dev session (desktop + android)
just build        # Windows-only: full dev release matrix
just check        # parallel pre-release gate
just check-changed --staged  # changed-file gate used by hooks/CI
```

## Android

```bash
just android                       # clean-start USB HMR session
just android host                  # bootstrap Wi-Fi/LAN session
just android usb R5CXB2PGC2H       # pick a device when multiple are attached
just dev stop                      # stop session and forwards

bun scripts/android-install.ts          # APK-only build + install
bun scripts/android-install.ts --force  # uninstall + reinstall (handles signature mismatch)
bun scripts/android-emu.ts              # headless x86_64 emulator
```

The `.vscode/tasks.json` entries "android: build + install APK" and "android: build + reinstall APK (wipe data)" wrap the install script.

## Live-session signals

- HMR proof after JS/CSS edit: `[vite] hmr update /src/...` in `tmp/android-dev.log`.
- USB physical devices still follow Tauri 2.11 mobile behaviour: on Windows, `tauri android dev` rewrites `localhost` to the host LAN IP and sets `TAURI_DEV_HOST`; Vite serves HMR on `1421` from that host. `adb reverse` is kept for localhost/emulator fallback.
- Crash/OOM proof: `am_kill` / `am_proc_died` / `am_crash` in `tmp/logcat.log` for the app.
- `location.href` is not a health signal on Android.

Full `tmp/` artefact contract: [`tmp.md`](tmp.md).

`just android` is repeatable but not a no-op: it stops any existing Android dev session before starting a new one.

## HMR rule

`src/**` edits hot-reload. Any backend / native / config / capability edit:

```bash
just dev stop && just android
```

While `tmp/_pids.json` exists and Vite owns `:1420`, do not run `cargo xtask android build`, `bun scripts/android-install.ts`, `cargo tauri build`, or any release build — each replaces the debug-dev APK and strands HMR.
