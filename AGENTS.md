# AGENTS.md

Stack: Tauri 2 · Rust edition 2024 (MSRV 1.85) · Vue 3 + TS + Vite · Bun · `just` (wraps `cargo xtask`).

## Layout

```
src/                  Vue 3 frontend (api.ts, types.ts mirrors Rust)
src-tauri/src/        commands.rs, lib.rs (invoke_handler), bin/wt.rs (CLI), config/models/paths/error/transcriber/
xtask/src/            build / release / android orchestration
scripts/              cdp.mjs, error-monitor.mjs, dev-bootstrap.ps1, diarize.py, install-*.ps1
docs/                 android · agents · dev-loop · release · rust-build-speed
.pi/agents/           diagnose · runner
```

## Commands

```
just dev                 desktop (HMR)
just android-dev[-host]  Android USB / LAN (HMR)
just check               pre-release gate (fmt + clippy + vue-tsc + tests + machete + audit)
just release-stable      check + bump + tag + build + publish
```

`just --list` for the rest.

## Conventions

- Rust edition 2024 (`LazyLock`, `let-else`); errors crossing JS use `error::Error` (`Serialize`).
- `src/types.ts` mirrors Rust structs. TS/Vue imports use aliases (`@/`, `@components/`, `@composables/`, `@utils/`, `@styles/`).
- New Tauri command = `commands.rs` handler + `lib.rs` `invoke_handler![…]` + `api.ts` wrapper + `types.ts` mirror.
- No comments in code. No `sleep` in scripts; poll a real signal with timeout.
- Conventional commits, simple British English.

## Pre-commit hook

`.githooks/pre-commit` is mandatory; `--no-verify` is forbidden. The hook auto-formats touched files (Rust via `cargo fmt`, TS/Vue/docs via `prettier --write`) and re-stages them, then gates on `bun run typecheck` for TS/Vue. Heavy checks (`clippy`, `cargo check`, full tests) run in `just check` / CI, not on every commit.

## Agent roster

Two agents, both opus, both read-only on the repo. Selection: _diagnosing a failure? installing/testing on a device?_ Everything else (search, edits, commits, code review) is done by the main thread, optionally with built-in `Explore` / `Plan` / `general-purpose` and the project's `code-review` / `simplify` / `security-review` skills.

| Agent         | Job                                                                                      | Writes                                                                   | Forbidden                                                        |
| ------------- | ---------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ | ---------------------------------------------------------------- |
| `wt-diagnose` | Root-cause one failing signal from `tmp/*.log` + `adb`/`netstat`/`tasklist`/`git log -p` | `tmp/diagnose-<slug>.md`                                                 | edits, builds, commits, device installs, agent-to-agent calls    |
| `wt-runner`   | Install + 30 s smoke test on Win GUI / Win CLI / Android / WSL CLI                       | `tmp/install-report.json`, `tmp/test-report.json` + install side-effects | git, source edits, dev-session APK rebuild, agent-to-agent calls |

Both return only `VERDICT / EVIDENCE / FIX`. Raw logs stay in their notes/artefact files — never in chat.

## Live dev invariant

Desktop loads `http://localhost:1420/` (confirm via `tmp/error-monitor.log`, no `:1421 failed`). Android `location.href` is `http://tauri.localhost/`; liveness signal is fresh `connecting to 127.0.0.1:1420` from `RustStdoutStderr` in `tmp/android-dev.log`. Absent → restart bootstrap.

While a dev session is live (`tmp/_pids.json` exists and Vite owns `:1420`), do not run `just android-install`, `just android-build`, `cargo tauri build`, or any `wtranscriber` release build — each replaces the debug-dev APK and strands HMR.

## Bootstrap

```
powershell -ExecutionPolicy Bypass -File scripts/dev-bootstrap.ps1 -Platform <desktop|android-usb|android-host>
```

Handles hooks path, detached spawn, port-owner tracking, `adb reverse`, logcat, CDP forwarding. Exits non-zero if Vite fails to bind `:1420` or (Android) WebView fails to connect within 180 s. Writes `tmp/_platform`, `tmp/_pids.json`. After OK, run `wt-diagnose` on the latest log to confirm a clean start.

## Per-turn during a live dev session

- **Desktop**: line-count diff `tmp/error-monitor.log`; new lines → `wt-diagnose`.
- **Android**: line-count diff `tmp/logcat.log`; new `am_kill` / `am_proc_died` / `am_crash` → `wt-diagnose`.
- Android JS edit must show `[vite] hmr update` in `tmp/android-dev.log` for the touched file. Rust/native edit requires bootstrap restart.

## Decision table

| Need                                  | Action                                                       |
| ------------------------------------- | ------------------------------------------------------------ |
| Find code in the repo                 | Built-in `Explore` agent or main-thread `Grep`/`Glob`        |
| Diagnose a failing log/runtime signal | `wt-diagnose`                                                |
| Web research                          | Built-in `general-purpose` agent with `WebSearch`/`WebFetch` |
| Review a diff against conventions     | `code-review` skill                                          |
| Edit project files                    | Main thread (pre-commit hook is the gate)                    |
| Commit + push                         | Main thread (`git add <paths> && git commit && git push`)    |
| Install + smoke-test                  | `wt-runner` (modes: `install`, `test`, `install-and-test`)   |
| Release                               | `just release-stable` via `wt-runner`                        |

## Skills

`tauri-debugging` covers WebView inspector, CDP, logcat, IPC — load before debugging.
