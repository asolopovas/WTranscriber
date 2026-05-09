# AGENTS.md

Stack: Tauri 2 · Rust edition 2024 (MSRV 1.85) · Vue 3 + TS + Vite · Bun · `just` (wraps `cargo xtask`).

## Layout

```
src/                  Vue 3 frontend (api.ts, types.ts mirrors Rust)
src-tauri/src/        commands.rs, lib.rs (invoke_handler), bin/wt.rs (CLI), config/models/paths/error/transcriber/
src-tauri/            tauri.conf.json, capabilities/default.json, Cargo.toml, gen/android/
xtask/src/            build / release / android orchestration
scripts/              cdp.mjs, error-monitor.mjs, diarize.py, install-*.ps1
docs/                 android · agents · dev-loop · release · rust-build-speed
.pi/agents/           diagnose · runner
```

## Commands

```
just dev                 desktop (HMR)
just android-bootstrap    Android USB / LAN detached HMR + logcat + CDP
just android-status       bounded health check for adb / HMR / CDP
just android-stop         stop detached Android dev session
just android-debug-eval   evaluate JS in the live Android WebView
just check               pre-release gate (fmt + clippy + vue-tsc + tests + machete + audit)
just release-stable      check + bump + tag + build + publish
```

`just --list` for the rest.

## Conventions

- Rust edition 2024 (`LazyLock`, `let-else`); errors crossing JS use `error::Error` (`Serialize`).
- Respect Tauri's process split: Vue/WebView owns presentation; Rust core owns filesystem, model, native, and long-running work; cross the boundary only through commands/events.
- Use commands for request/response and events for progress streams or fire-and-forget notifications.
- `src/types.ts` mirrors Rust structs. TS/Vue imports use aliases (`@/`, `@components/`, `@composables/`, `@utils/`, `@styles/`).
- New Tauri command = `commands.rs` handler + `lib.rs` `invoke_handler![…]` + `api.ts` wrapper + `types.ts` mirror; add or tighten `src-tauri/capabilities/default.json` permissions when the frontend calls a new plugin/API.
- Keep capability permissions least-privilege. If IPC fails, inspect console + `RustStdoutStderr` before widening permissions.
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

## Tauri workflow by change type

| Change                              | Touch                                         | Fast verification                                     | Dev-session action                              |
| ----------------------------------- | --------------------------------------------- | ----------------------------------------------------- | ----------------------------------------------- |
| Vue / TS / CSS UI                   | `src/**`                                      | `bun run typecheck`; CDP eval for layout/state        | No restart; confirm Vite HMR line on Android    |
| Rust command / IPC shape            | `commands.rs`, `lib.rs`, `api.ts`, `types.ts` | Focused Rust test/check + `bun run typecheck`         | Android needs bootstrap restart                 |
| Rust long-running/native work       | `src-tauri/src/**`                            | Focused Rust test/check; inspect `RustStdoutStderr`   | Android needs bootstrap restart                 |
| Tauri config/capability/plugin use  | `tauri.conf.json`, `capabilities/*.json`      | Reproduce the exact invoke/API path; check IPC errors | Restart bootstrap                               |
| Android scaffold/resources/manifest | `src-tauri/gen/android/**`                    | Device smoke via `wt-runner` when install is needed   | Restart bootstrap; avoid release build mid-HMR  |
| Release/build orchestration         | `xtask/**`, `justfile`, `scripts/install-*`   | Targeted command, then `just check` before release    | Stop live dev first if it would replace the APK |

Prefer live inspection over screenshots: `just android-debug-attach`, then `node scripts/cdp.mjs "<expr>"` for DOM, computed styles, console state, and route checks. Use screenshots only for visual judgement.

## Live dev invariant

Desktop loads `http://localhost:1420/` (confirm via `tmp/error-monitor.log`, no `:1421 failed`). Android `location.href` is `http://tauri.localhost/`; liveness signal is fresh `connecting to 127.0.0.1:1420` from `RustStdoutStderr` in `tmp/android-dev.log`. Absent → restart bootstrap.

While a dev session is live (`tmp/_pids.json` exists and Vite owns `:1420`), do not run `just android-install`, `just android-build`, `cargo tauri build`, or any `wtranscriber` release build — each replaces the debug-dev APK and strands HMR.

## Bootstrap

```
just android-bootstrap usb
just android-bootstrap host
```

Handled by `cargo xtask android bootstrap`: detached spawn, port-owner tracking, `adb reverse`, logcat, and CDP forwarding. Exits non-zero if Vite fails to bind `:1420`, the WebView fails to connect within 180 s, or CDP cannot attach. Writes `tmp/_platform`, `tmp/_pids.json`. After OK, run `just android-status` for a bounded health check.

## Per-turn during a live dev session

- Capture the relevant log line count before work and compare it before responding.
- **Desktop**: line-count diff `tmp/error-monitor.log`; new lines → `wt-diagnose`.
- **Android**: line-count diff `tmp/logcat.log`; new `am_kill` / app `am_proc_died` / `am_crash` → `wt-diagnose`.
- Android JS edit must show `[vite] hmr update` in `tmp/android-dev.log` for the touched file. Rust/native/config/capability edit requires bootstrap restart.
- Do not use `location.href` as Android liveness; Tauri's custom scheme reports `http://tauri.localhost/` even when Vite/HMR is stale.

## Decision table

| Need                                  | Action                                                                                          |
| ------------------------------------- | ----------------------------------------------------------------------------------------------- |
| Find code in the repo                 | Built-in `Explore` agent or main-thread `Grep`/`Glob`                                           |
| Diagnose a failing log/runtime signal | `wt-diagnose`                                                                                   |
| Debug Tauri/WebView/IPC live          | Load `tauri-debugging`; use CDP + logcat/RustStdoutStderr                                       |
| Add or change Tauri command/API       | Main thread; keep Rust handler, invoke handler, API wrapper, TS types, and capabilities in sync |
| Web research                          | Built-in `general-purpose` agent with `WebSearch`/`WebFetch`                                    |
| Review a diff against conventions     | `code-review` skill                                                                             |
| Edit project files                    | Main thread (pre-commit hook is the gate)                                                       |
| Commit + push                         | Main thread (`git add <paths> && git commit && git push`)                                       |
| Install + smoke-test                  | `wt-runner` (modes: `install`, `test`, `install-and-test`)                                      |
| Release                               | `just release-stable` via `wt-runner`                                                           |

## Skills

- `tauri` covers architecture, IPC commands/events, capabilities, plugins, mobile, and distribution — load before changing Tauri structure or workflow.
- `tauri-debugging` covers WebView inspector, CDP, logcat, IPC — load before debugging.
