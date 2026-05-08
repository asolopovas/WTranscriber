# Dev loop

Edit → see it on the device instantly → get told the moment something breaks. Three concurrent processes.

## 1. HMR dev session - user's terminal

```
just android-dev          # USB / emulator
just android-dev-host     # Wi-Fi / LAN
```

Frontend-only watcher (`--no-watch`). Vue/TS/CSS push live; Rust edits ignored. Backend rebuilds via `just android-install` in another terminal - HMR keeps streaming, app relaunches with new native code.

Run in the user's own terminal. Spawning through a subagent on Windows pops an empty conhost (`CREATE_NEW_PROCESS_GROUP` quirk).

### Detached spawn (orchestrator)

When the orchestrator launches `just dev` / `just android-dev[-host]` or `node scripts/error-monitor.mjs`, use PowerShell `Start-Process` so the child outlives the agent turn:

```bash
powershell -Command "Start-Process -FilePath 'just' -ArgumentList 'android-dev' \
  -RedirectStandardOutput 'C:\Users\asolo\src\WTranscriber\tmp\android-dev.log' \
  -RedirectStandardError  'C:\Users\asolo\src\WTranscriber\tmp\android-dev.err.log' \
  -WorkingDirectory       'C:\Users\asolo\src\WTranscriber' \
  -WindowStyle Hidden -PassThru | Select-Object Id"
```

Confirm liveness with `tasklist //FI "PID eq <id>"`; shut down with `taskkill //F //PID <id>`. Same pattern for the error monitor (`tmp/error-monitor.log`).

`Start-Process` does **not** propagate the parent's `$env:VAR` to the child. To pass env vars (e.g. `TAURI_DEV_HOST=127.0.0.1`), wrap via `cmd /c` and a one-shot `.cmd` file:

```
# tmp/_dev.cmd
set TAURI_DEV_HOST=127.0.0.1 && just android-dev
```

```bash
powershell -Command "Start-Process cmd.exe -ArgumentList '/c','tmp\_dev.cmd' \
  -RedirectStandardOutput 'tmp\android-dev.log' \
  -RedirectStandardError  'tmp\android-dev.err.log' \
  -WindowStyle Hidden -PassThru | Select-Object Id"
```

The returned PID is the `cmd.exe` wrapper and exits as soon as the inner pipeline launches. Track the long-running server via the port owner: `netstat -ano | findstr :1420`.

## 2. CDP attach - once, after launch

```
just android-debug-attach
```

Forwards `tcp:9222` to the WebView. Required for live JS eval and the error monitor.

```
node scripts/cdp.mjs "<expr>"
```

`getBoundingClientRect`, `getComputedStyle`, `outerHTML`, `querySelectorAll`, anything. Use this instead of PNG screenshots for layout/spacing/colors/classes.

## 3. Error monitor - async subagent

```
node scripts/error-monitor.mjs
```

Captures (deduped, noise-filtered):

- **Logcat `*:W`** - all `E`/`F`, `RustStdoutStderr` ERROR/WARN/panic, native crashes (`AndroidRuntime`, `tombstoned`).
- **CDP runtime** - every JS `console.error`/`console.warn`, uncaught `pageerror` with stack, failed network requests.
- Drops: reqwest/hyper, HwcComposer, SurfaceFlinger, SemGameManager, `setRequestedFrameRate`, BufferQueue, ViewRootImpl.
- Burst-dedup (2s window).
- Writes stdout + `tmp/error-monitor.log` (gitignored).

Spawn as long-running async delegate so the inactivity timeout never kills it:

```
subagent({
  agent: "delegate",
  task: "node scripts/error-monitor.mjs\n\nStream forever. Surface any error/warn line back as a concise message. Ignore inactivity warnings.",
  async: true,
  cwd: "C:/Users/asolo/src/WTranscriber",
  control: { enabled: false },
})
```

The monitor reattaches to a new WebView instance automatically after `just android-install` (CDP retries ~2 min).

### Live signals on Android

`scripts/error-monitor.mjs` and `tmp/observer-latest.json` are **desktop-only**. On Android, OOM kills and process-death events appear in the Android events log buffer, not the main buffer that `error-monitor.mjs` tails. `tmp/observer-latest.json` will show a stale counter from any previous desktop session.

Authoritative Android live-signal source: `tmp/logcat.log`, populated at bootstrap by:

```
adb logcat -c
adb logcat -b main,events *:W RustStdoutStderr:V Tauri:V chromium:V am_crash:V am_proc_died:V am_kill:V
```

Key tags:

| Tag                | What it signals                            |
| ------------------ | ------------------------------------------ |
| `am_kill`          | Low-memory killer (OOM) terminated the app |
| `am_proc_died`     | Process death (any cause)                  |
| `am_crash`         | Unhandled Java/Kotlin crash                |
| `RustStdoutStderr` | Rust `println!` / `eprintln!` / panics     |
| `chromium`         | WebView JS errors and console output       |
| `Tauri`            | Tauri IPC and plugin events                |

Spawn as a detached process at bootstrap (same `Start-Process` pattern as the dev server) and record its PID. The per-turn diff target on Android is the line-count of `tmp/logcat.log`, **not** `tmp/error-monitor.log`.

Agent roster, decision table, and delegation rules live in [`AGENTS.md`](../AGENTS.md) - not restated here.
