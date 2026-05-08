# AGENTS.md

Stack: Tauri 2 · Rust edition 2024 (MSRV 1.85) · Vue 3 + TS + Vite · Bun · `just` (wraps `cargo xtask`).

## Layout

```
src/                  Vue 3 frontend (api.ts, types.ts mirrors Rust)
src-tauri/src/        commands.rs, lib.rs (invoke_handler), bin/wt.rs (CLI), config/models/paths/error/transcriber/
xtask/src/            build / release / android orchestration
scripts/              cdp.mjs, error-monitor.mjs, diarize.py, install-*.ps1
docs/                 android · agents · dev-loop · release · rust-build-speed
.pi/agents/           coder · committer · runner · triage · scout · researcher · docs-updater
```

## Commands

```
just dev                 desktop (HMR)
just android-dev[-host]  Android USB / LAN (HMR)
just check               pre-release gate (fmt + clippy + vue-tsc + tests + machete + audit)
just release-stable      check + bump + tag + build + publish
```

`just --list` for the rest. Pre-commit hook (`.githooks/pre-commit`) is scoped to staged files and mandatory; `--no-verify` is forbidden.

## Conventions

- Rust edition 2024 (`LazyLock`, `let-else`). Errors crossing the JS boundary use `error::Error` (`Serialize`).
- `src/types.ts` mirrors Rust structs.
- TS/Vue imports use path aliases (`@/`, `@components/`, `@composables/`, `@utils/`, `@styles/`).
- New Tauri command: `commands.rs` handler + `lib.rs` `invoke_handler![…]` + `api.ts` wrapper + `types.ts` mirror.
- No comments in code. No `sleep` in scripts; poll a real signal with timeout.
- Conventional commits, simple British English.

## Testing

- Inner loop: `cargo check -p wtranscriber`, `bunx vue-tsc --noEmit`.
- Pre-release gate: `just check` (route through `wt-triage`, never main thread).
- Cross-platform smoke: chain `install-and-test` (30 s clip → `tmp/install-report.json` + `tmp/test-report.json`).
- Skill `tauri-debugging` (global) covers WebView inspector, CDP, logcat, IPC. Load before any debugging session.

## Orchestrator runbook

> **Live dev invariant.** The device WebView must load from the Vite dev URL throughout any development session. Verify with `just android-debug-eval "location.href"` (Android) or the WebView inspector (desktop). A result containing `tauri.localhost` means the device is on a stale bundled build — kill the installed APK, restart the dev recipe, and re-verify before marking any change live.

Main thread coordinates only: design + delegate + synthesise. Never edits source, greps logs, runs `just check`, or commits.

### Bootstrap (per session)

1. `git config --get core.hooksPath` must be `.githooks`. Set if not. Ensure `tmp/` exists.
2. Ask platform if unknown (desktop / android USB / android Wi-Fi). Recipe is fixed per transport; switching mid-session strands HMR (reload via CDP first):
   - Desktop: `just dev`. Android USB: `just android-dev`. Android Wi-Fi: `just android-dev-host`.
   - Android: after WebView is up, `just android-debug-attach` forwards CDP `:9222`.
3. Spawn dev server + monitor as detached Windows processes (PowerShell `Start-Process`, see `docs/dev-loop.md`). Record PIDs; when a `cmd /c` wrapper is used the wrapper PID exits immediately, so track the port-owning PID via `netstat -ano | findstr :1420`. Write `tmp/_platform` as `android` or `desktop`. **Android only**: additionally spawn `adb logcat -c` then `adb logcat -b main,events *:W RustStdoutStderr:V Tauri:V chromium:V am_crash:V am_proc_died:V am_kill:V` → `tmp/logcat.log` as a detached process; record its PID (see `docs/dev-loop.md` §Live signals on Android).
4. HMR sanity check via `wt-triage`: CDP target URL correct, `tmp/error-monitor.log` clean of `:1421 failed` **(desktop)** / `tmp/logcat.log` free of `am_kill`/`am_proc_died` **(Android)**, no `Replacing devUrl host` substitution to a non-loopback address, touched `src/main.ts` triggers `[vite] hot updated`. Ports 1420/1421 must be free before relaunch.
5. **Android only**: run `just android-debug-eval "location.href"`. If the result contains `tauri.localhost` or any non-Vite host, bootstrap FAILED — restart the dev recipe from step 3. Record the verified URL in the checklist.
6. Report bootstrap as a checklist.

Never instruct the user to run a dev command; the orchestrator launches it.

### Per-turn

**Desktop**: diff `tmp/error-monitor.log` line count. **Android**: diff `tmp/logcat.log` line count. New lines → `wt-triage` with excerpt. Then Decision table.

**Android — after every edit batch**: JS edit: confirm `tmp/android-dev.log` contains `[vite] hmr update` for the touched file since the edit timestamp. Rust edit: confirm `just android-dev` was restarted and `just android-debug-eval "location.href"` still returns the Vite URL.

### Decision table

| Signal                                                                                 | Action                                                                                                                                                                                                                                                                                      |
| -------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| New line in `tmp/error-monitor.log` (desktop)                                          | `wt-triage` with excerpt                                                                                                                                                                                                                                                                    |
| New line in `tmp/logcat.log` (Android) — incl. `am_kill` / `am_proc_died` / `am_crash` | `wt-triage` with logcat excerpt                                                                                                                                                                                                                                                             |
| Source edit needed                                                                     | `wt-coder`                                                                                                                                                                                                                                                                                  |
| Edits applied, gate not hit                                                            | `wt-committer`                                                                                                                                                                                                                                                                              |
| Need to find where X lives in repo                                                     | `wt-scout`                                                                                                                                                                                                                                                                                  |
| Need a built artifact                                                                  | `wt-runner` (mode: install)                                                                                                                                                                                                                                                                 |
| Rust/native edit during live dev (Android)                                             | Kill dev session; re-run `just android-dev` (rebuilds + reinstalls the debug-dev APK that loads the Vite dev URL). Never `just android-install` / `just android-build` / `wt-runner` install — all replace the dev APK with a bundled-assets build and silently strand subsequent JS edits. |
| JS-only edit on Android                                                                | No reinstall. Verify: `tmp/android-dev.log` shows `[vite] hmr update /src/…` for the touched file; `just android-debug-eval "location.href"` returns the Vite URL, not `tauri.localhost`.                                                                                                   |
| Specific in-app misbehaviour or gate failure (single signal)                           | `wt-triage`                                                                                                                                                                                                                                                                                 |
| Continuous live-signal watch (desktop)                                                 | `scripts/observer.mjs` running; poll `tmp/observer-latest.json`. **Desktop-only — blind on Android.**                                                                                                                                                                                       |
| Continuous live-signal watch (Android)                                                 | tail `tmp/logcat.log` (detached `adb logcat` spawned at bootstrap); `tmp/observer-latest.json` counter is stale/blind on Android                                                                                                                                                            |
| 30 s-clip smoke after install                                                          | chain `install-and-test`                                                                                                                                                                                                                                                                    |
| Commit / ship                                                                          | `wt-committer`                                                                                                                                                                                                                                                                              |
| Release                                                                                | `wt-committer` → `just release-stable` artifacts via `wt-runner`                                                                                                                                                                                                                            |
| External knowledge needed                                                              | `wt-researcher`                                                                                                                                                                                                                                                                             |
| Recurring agent failure or workflow drift                                              | `wt-docs-updater` → `wt-committer` as `chore(agents): tighten <name>`                                                                                                                                                                                                                       |

### Coordination

- **Fresh context per worker** (`defaultContext: fresh`). Orchestrator carries project state; workers re-derive.
- **File-signal contract** under `tmp/`: every worker writes a JSON or `.md` artifact (`coder-report`, `last-commit`, `install-report`, `test-report`, `triage-<topic>`, `scout-<slug>`, `research-<slug>`, `docs-update`, `error-monitor.log`). Workers never read each other's stdout.
- **Chain when dependent, parallel when independent.** Chains live in `.pi/chains/`. Current: `install-and-test`. For cross-file work the orchestrator may chain `wt-scout → wt-coder → wt-committer`; skip any link not warranted.
- Cross-file refactor planning is orchestrator's job (per runbook). Pre-commit semantic review: dispatch `wt-triage` with `mode: review` on the staged diff.

### Hard prohibitions

- No `git`, `cargo`, `bun`, `just check`, or log probing (`tail`/`grep`/`adb`/`curl`/`tasklist`) from main thread; poll `tmp/observer-latest.json` (desktop) or `tmp/logcat.log` line-count (Android), or dispatch `wt-triage`.
- On Android, do not use `tmp/observer-latest.json` or `tmp/error-monitor.log` as the live-signal source; they are blind to OOM and process-death events. Use `tmp/logcat.log` (or dispatch `wt-triage` with a logcat tail) instead.
- Never run `just android-install`, `just android-build`, or `wt-runner` install mode during a dev session. All three replace the debug-dev APK with a bundled-assets build that ignores Vite. The only Android-side reinstall path during dev is restarting `just android-dev`.
- Do not declare a JS or Rust change live on Android without verifying via `just android-debug-eval` (HMR entry in `tmp/android-dev.log` for JS; fresh `just android-dev` start for Rust).
- No agent-to-agent calls; signal via `tmp/` files.
- No raw log dumps to the user; relay `VERDICT / EVIDENCE / FIX` only.

### Self-repair

Tests red or build broken → re-delegate with a sharper spec. Agent misbehaves (missing contract block, raw-log dump, gate bypass, agent-to-agent call, scope creep) → `wt-docs-updater` patches the smallest closing rule. Two failed repair attempts → escalate to the user.

Agent quality bar: `.pi/agents/wt-docs-updater.md`.
