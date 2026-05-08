# Dev loop

Edit → see it on the device instantly → get told the moment something breaks. Three concurrent processes.

## 1. HMR dev session — user's terminal

```
just android-dev          # USB / emulator
just android-dev-host     # Wi-Fi / LAN
```

Frontend-only watcher (`--no-watch`). Vue/TS/CSS push live; Rust edits ignored. Backend rebuilds via `just android-install` in another terminal — HMR keeps streaming, app relaunches with new native code.

Run in the user's own terminal. Spawning through a subagent on Windows pops an empty conhost (`CREATE_NEW_PROCESS_GROUP` quirk).

## 2. CDP attach — once, after launch

```
just android-debug-attach
```

Forwards `tcp:9222` to the WebView. Required for live JS eval and the error monitor.

```
node scripts/cdp.mjs "<expr>"
```

`getBoundingClientRect`, `getComputedStyle`, `outerHTML`, `querySelectorAll`, anything. Use this instead of PNG screenshots for layout/spacing/colors/classes.

## 3. Error monitor — async subagent

```
node scripts/error-monitor.mjs
```

Captures (deduped, noise-filtered):

- **Logcat `*:W`** — all `E`/`F`, `RustStdoutStderr` ERROR/WARN/panic, native crashes (`AndroidRuntime`, `tombstoned`).
- **CDP runtime** — every JS `console.error`/`console.warn`, uncaught `pageerror` with stack, failed network requests.
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

## Project subagents (`.pi/agents/`)

| Agent          | Purpose                                                                                                                                                      |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `doctor`       | Commits/pushes (pre-commit gate + conventional message) and diagnostic forensics (tests, CDP, logcat, monitor logs). Returns `VERDICT` / `EVIDENCE` / `FIX`. |
| `wt-installer` | End-user install verification (Windows GUI+CLI, Android, WSL).                                                                                               |
| `wt-tester`    | Functional smoke test with a 30-second clip across all platforms.                                                                                            |

### Delegation rules

- **Don't grep logs in main thread.** Hand `doctor` a focused question; it reads `tmp/error-monitor.log`, runs CDP probes, returns a verdict.
- **Don't run `just check` in main thread.** `doctor` absorbs the multi-minute output and surfaces only decisions.
- **All commits go through `doctor`.** Pass the change summary; it stages, gates, writes the message, pushes, returns the hash.
