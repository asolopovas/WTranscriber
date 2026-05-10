# AGENTS.md

Stack: Tauri 2 · Rust edition 2024 (MSRV 1.85) · Vue 3 + TS + Vite · Bun · `just`.

## Layout

```
src/             Vue 3 frontend (api.ts, types.ts mirrors Rust)
src-tauri/src/   commands/ (per-domain), lib.rs (invoke_handler!), bin/wt.rs, api.rs, config.rs, paths.rs, error.rs, models/, transcriber/, diarizer/, audio/, runtimes/, llm/, engine/
src-tauri/       tauri.conf.json, capabilities/default.json, gen/android/
xtask/src/       bump / publish / release / android orchestration
scripts/         run.mjs, parallel.mjs, android-emu.mjs, cdp.mjs, lint-vue.mjs, clean-temp.mjs, install-*.ps1
docs/            android · dev-loop · release · rust-build-speed
.pi/agents/      diagnose · runner
```

## Task contract

Every `just` recipe runs through `scripts/run.mjs`: line-prefixed output, heartbeat after 10 s of silence, kill on idle (default 90 s) or hard timeout (default 600 s), final `OK in X.Ys` / `FAIL exit=N in X.Ys`. Long-running interactive recipes (`dev`, `dev-cpu`, `watch`) use `--idle 0 --max 0`; `just android` is finite (it bootstraps a detached session and exits). Anything quiet >30 s is a bug.

## Commands

```
just dev               desktop HMR (Linux/Windows)
just android           Android USB/emu HMR session (idempotent)
just android-stop      stop the session
just android-emu       headless x86_64 emulator (cross-platform)
just check             parallel pre-release gate
just release-stable    check + bump + tag + build + publish
```

`just check` runs in parallel via `scripts/parallel.mjs`: `fmt-check`, `clippy`, `typecheck`, `vue-lint`, `rust-test`, `js-test`, `machete`, `audit`. First failure wins; all jobs complete. Sequential variants exist for targeted runs (`just lint`, `just test`, …).

`just --list` for the rest.

## Conventions

- Rust edition 2024 (`LazyLock`, `let-else`); errors crossing JS use `error::Error` (`Serialize`).
- Tauri process split: Vue/WebView owns presentation; Rust owns filesystem, models, native, long work. Cross only via commands/events.
- `src/types.ts` mirrors Rust structs. Use aliases `@/`, `@components/`, `@composables/`, `@utils/`, `@styles/`.
- New Tauri command = `commands/<domain>.rs` handler + `lib.rs` `invoke_handler![…]` (full path) + `api.ts` wrapper + `types.ts` mirror + `src-tauri/capabilities/default.json` permission if it touches a plugin/API.
- Capability permissions: least-privilege. If IPC fails, inspect console + `RustStdoutStderr` before widening.
- No comments in code. No `sleep` in scripts; poll with timeout.
- Conventional commits, simple British English.

## Pre-commit hook

`.githooks/pre-commit` is mandatory; `--no-verify` is forbidden. Auto-formats touched files (Rust via `cargo fmt`, TS/Vue/docs via `prettier --write`), re-stages, then gates on `bun run typecheck`. Heavy checks live in `just check`.

## Live dev invariant

- Desktop: Vite owns `http://localhost:1420/`. The live `[dev]` stream from `just dev` is the source of truth; a `:1421 failed` / `EADDRINUSE` line there means HMR is dead.
- Android: liveness = fresh `connecting to 127.0.0.1:1420` in `tmp/logcat.log` (`RustStdoutStderr`). `location.href` is **not** a signal — Tauri reports `http://tauri.localhost/` even when HMR is stale.
- While `tmp/_pids.json` exists and Vite owns `:1420`, do **not** run `just android-install`, `just android-build`, `cargo tauri build`, or any release build — each replaces the debug-dev APK and strands HMR.

## Per-turn during a live dev session

- Desktop: scan the `[dev]` stream for new error/panic lines. Android: diff `tmp/logcat.log` line counts. New failures → `wt-diagnose`.
- Android JS edit must show `[vite] hmr update` in `tmp/android-dev.log`. Rust/native/config/capability edit requires `just android-stop && just android`.
- New `am_kill` / `am_proc_died` / `am_crash` for the app → `wt-diagnose`.

## Tauri workflow by change type

| Change                        | Touch                                                  | Verify                                              | Session action               |
| ----------------------------- | ------------------------------------------------------ | --------------------------------------------------- | ---------------------------- |
| Vue / TS / CSS                | `src/**`                                               | `bun run typecheck`; CDP eval                       | No restart; confirm HMR line |
| Rust command / IPC shape      | `commands/<domain>.rs`, `lib.rs`, `api.ts`, `types.ts` | Focused Rust test/check + typecheck                 | Android: restart bootstrap   |
| Rust native / long-running    | `src-tauri/src/**`                                     | Focused Rust test/check; inspect `RustStdoutStderr` | Android: restart bootstrap   |
| Tauri config / capability     | `tauri.conf.json`, `capabilities/*.json`               | Reproduce the exact invoke; check IPC errors        | Restart bootstrap            |
| Android scaffold / manifest   | `src-tauri/gen/android/**`                             | Device smoke via `wt-runner`                        | Restart bootstrap            |
| Release / build orchestration | `xtask/**`, `justfile`, `scripts/install-*`            | Targeted command, then `just check`                 | Stop live dev first          |

## Agent roster

| Agent         | Job                                                                 | Writes                                                           | Forbidden                                        |
| ------------- | ------------------------------------------------------------------- | ---------------------------------------------------------------- | ------------------------------------------------ |
| `wt-diagnose` | Root-cause one failing signal from `tmp/*.log` + `adb`/`git log -p` | `tmp/diagnose-<slug>.md`                                         | edits, builds, commits, installs, agent-to-agent |
| `wt-runner`   | Install + 30 s smoke test on Win GUI / Win CLI / Android / WSL CLI  | `tmp/install-report.json`, `tmp/test-report.json` + side-effects | git, source edits, dev-session APK rebuild       |

Both return only `VERDICT / EVIDENCE / FIX`. Raw logs stay in artefact files.

## Decision table

| Need                          | Action                                                                |
| ----------------------------- | --------------------------------------------------------------------- |
| Find code                     | Main-thread `Grep`/`Glob`, or `Explore` agent                         |
| Diagnose a failing log signal | `wt-diagnose`                                                         |
| Debug Tauri/WebView/IPC live  | Skill `tauri` (debugging section); CDP + logcat/`RustStdoutStderr`    |
| Add/change Tauri command      | Main thread; sync handler + invoke + api.ts + types.ts + capabilities |
| Edit project files            | Main thread (pre-commit hook is the gate)                             |
| Install + smoke-test          | `wt-runner` (modes: `install`, `test`, `install-and-test`)            |
| Release                       | `just release-stable` via `wt-runner`                                 |

## Skills

- `tauri` — load before architectural / IPC / capability / mobile / distribution changes, and for WebView/CDP/logcat debugging.
