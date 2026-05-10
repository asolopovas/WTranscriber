# Dev loop

## Task contract

Every `just` recipe runs through `scripts/run.ts`:

- Output line-prefixed with `[tag]`.
- Heartbeat after 10 s of silence: `… still running, Xs elapsed, Ys without output`.
- Idle timeout (`--idle`, default 90 s): kills with `FAIL IDLE_TIMEOUT`, exit 124.
- Hard timeout (`--max`, default 600 s): kills with `FAIL MAX_TIMEOUT`, exit 124.
- Final summary: `OK in X.Ys` / `FAIL exit=N in X.Ys`.

Long-running interactive recipes (`dev`, `dev-cpu`, `watch`) use `--idle 0 --max 0` (heartbeat only); `just android` is finite (bootstraps the detached session and exits). Anything quiet >30 s is a bug.

`just check` runs **8 gates** in parallel via `scripts/parallel.ts`: `fmt-check`, `clippy`, `typecheck`, `vue-lint`, `rust-test`, `js-test`, `machete`, `audit`. First failure wins; all complete. The same recipe runs in CI on every push and PR.

## Desktop

Linux and Windows are the supported hosts. macOS works via Tauri (`bundle.targets` includes `app`) but is not part of the release matrix.

```bash
just dev          # HMR
just dev-cpu      # HMR with sherpa-static (no CUDA)
just build-app    # fast no-bundle build
just build        # full bundle (NSIS on Windows, .deb on Linux, .app on macOS)
just check        # parallel pre-release gate
just e2e          # Playwright UI tests (Vite + mocked Tauri IPC)
```

## Android

```bash
just android                  # bootstrap USB/emu HMR session (idempotent)
just android-host             # bootstrap Wi-Fi/LAN session
just android-status           # bounded health check (≤30 s)
just android-status-json      # machine-readable health
just android-smoke            # fail-fast end-to-end probe
just android-stop             # stop session and forwards
just android-debug-eval "document.title"
just android-emu              # cross-platform headless x86_64 emulator
just android-emu-stop
```

Pass a device serial when multiple are attached: `just android R5CXB2PGC2H`.

## Live-session signals

- HMR proof after JS/CSS edit: `[vite] hmr update /src/...` in `tmp/android-dev.log`.
- Crash/OOM proof: `am_kill` / `am_proc_died` / `am_crash` in `tmp/logcat.log` for the app.
- `location.href` is not a health signal on Android.

Full `tmp/` artefact contract: [`tmp.md`](tmp.md).

## HMR rule

`src/**` edits hot-reload. Any backend / native / config / capability edit:

```bash
just android-stop && just android
```

Never run `just android-install`, `just android-build`, `cargo tauri build`, or any release build while `tmp/_pids.json` exists and Vite owns `:1420`.
