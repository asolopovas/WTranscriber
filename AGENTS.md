# AGENTS.md

Stack: Tauri 2 · Rust edition 2024 (MSRV 1.85) · Vue 3 + TS + Vite · Bun · `just` (wraps `cargo xtask`).

## Layout

```
src/                  Vue 3 frontend (api.ts, types.ts mirrors Rust)
src-tauri/src/        commands.rs, lib.rs (invoke_handler), bin/wt.rs (CLI), config/models/paths/error/transcriber/
xtask/src/            build / release / android orchestration
scripts/              cdp.mjs, error-monitor.mjs, dev-bootstrap.ps1, diarize.py, install-*.ps1
docs/                 android · agents · dev-loop · release · rust-build-speed
.pi/agents/           investigate · edit · runner
```

## Commands

```
just dev                 desktop (HMR)
just android-dev[-host]  Android USB / LAN (HMR)
just check               pre-release gate (fmt + clippy + vue-tsc + tests + machete + audit)
just release-stable      check + bump + tag + build + publish
```

`just --list` for the rest. Pre-commit hook (`.githooks/pre-commit`) is mandatory; `--no-verify` is forbidden.

## Conventions

- Rust edition 2024 (`LazyLock`, `let-else`); errors crossing JS use `error::Error` (`Serialize`).
- `src/types.ts` mirrors Rust structs. TS/Vue imports use aliases (`@/`, `@components/`, `@composables/`, `@utils/`, `@styles/`).
- New Tauri command = `commands.rs` handler + `lib.rs` `invoke_handler![…]` + `api.ts` wrapper + `types.ts` mirror.
- No comments in code. No `sleep` in scripts; poll a real signal with timeout.
- Conventional commits, simple British English.

## Agent roster

Three agents, one verb each, no scope overlap. Selection: _reading? mutating? touching a device?_

| Agent            | Verb                  | Owns                                                                                                                                                                                           | Forbidden                                    |
| ---------------- | --------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------- |
| `wt-investigate` | answer (read-only)    | `mode: map` locate, `mode: diagnose` root-cause one signal, `mode: research` web, `mode: review` diff                                                                                          | edits, builds, git, device installs          |
| `wt-edit`        | mutate files          | `mode: edit` (default): smallest spec-conforming edit + scoped checks. `mode: finalise` (explicit only): stage named paths, commit through pre-commit hook, push, write `tmp/last-commit.json` | `--no-verify`, `just check`, device installs |
| `wt-runner`      | install + test device | install + smoke-test on Win GUI, Win CLI, Android, WSL                                                                                                                                         | git, source edits, dev-session APK rebuild   |

Sequencing: `wt-investigate` → `wt-edit` → optional `wt-runner` → `wt-edit mode: finalise` only when orchestrator names exact paths. Default `wt-edit` never touches git. No agent-to-agent calls. Skill `tauri-debugging` covers WebView inspector, CDP, logcat, IPC — load before debugging.

## Orchestrator runbook

Main thread: design + delegate + synthesise only. Never edits source, runs `git`/`cargo`/`bun`/`just check`/`adb`, or greps logs.

**Live dev invariant.** Desktop loads `http://localhost:1420/` (confirm via `tmp/error-monitor.log`, no `:1421 failed`). Android `location.href` is `http://tauri.localhost/`; liveness signal is fresh `connecting to 127.0.0.1:1420` from `RustStdoutStderr` in `tmp/android-dev.log`. Absent → restart bootstrap.

### Bootstrap

```
powershell -ExecutionPolicy Bypass -File scripts/dev-bootstrap.ps1 -Platform <desktop|android-usb|android-host>
```

Handles hooks path, detached spawn, port-owner tracking, `adb reverse`, logcat, CDP forwarding. Exits non-zero if Vite fails to bind `:1420` or (Android) WebView fails to connect within 180 s. Writes `tmp/_platform`, `tmp/_pids.json`. After OK, dispatch `wt-investigate mode: diagnose` on the latest log to confirm a clean start.

### Per-turn

- **Desktop**: line-count diff `tmp/error-monitor.log`; new lines → `wt-investigate mode: diagnose`.
- **Android**: line-count diff `tmp/logcat.log`; new `am_kill` / `am_proc_died` / `am_crash` → `wt-investigate mode: diagnose`.
- Android JS edit must show `[vite] hmr update` in `tmp/android-dev.log`; Rust/native edit requires bootstrap restart. `just android-install`, `just android-build`, `wt-runner install` forbidden during a dev session (bundled-assets APK strands HMR).

### Decision table

| Need                                | Dispatch                                                                                         |
| ----------------------------------- | ------------------------------------------------------------------------------------------------ |
| Find/diagnose/research/review       | `wt-investigate` (pick mode)                                                                     |
| Edit any project file               | `wt-edit` (default `mode: edit`)                                                                 |
| Commit + push completed change      | `wt-edit mode: finalise` with exact paths                                                        |
| Build artefact, install, smoke-test | `wt-runner`                                                                                      |
| 30 s smoke after install            | chain `install-and-test`                                                                         |
| Release                             | `wt-edit mode: finalise` → `just release-stable` via `wt-runner`                                 |
| Recurring agent/workflow drift      | `wt-edit` on `.pi/agents/**` or `AGENTS.md`, then `wt-edit mode: finalise` as `chore(agents): …` |

### Artefact contract

All workers `defaultContext: fresh`; re-derive state from logs and `git log`. Each worker writes its `tmp/` artefact; the next worker reads it. Missing artefact = no run; re-dispatch. Artefacts: `investigate-<slug>.md`, `edit-report.json`, `last-commit.json`, `install-report.json`, `test-report.json`. Workers never read each other's stdout or call each other. Orchestrator relays only `VERDICT / EVIDENCE / FIX`, never raw logs. Chain when dependent, parallel when independent.

### Watchdog

Any `wt-edit` / `wt-runner` / `wt-investigate mode: research` expected > 60 s runs async with:

```
async: true,
control: {
  enabled: true,
  activeNoticeAfterMs: 180000,
  needsAttentionAfterMs: 300000,
  failedToolAttemptsBeforeAttention: 2,
  notifyChannels: ["event", "async"],
  notifyOn: ["active_long_running", "needs_attention"]
}
```

On `needs_attention`: `subagent action: status id: <prefix>`, then `action: interrupt` if no progress, then re-dispatch with smaller scope. Synchronous `Failed` = hang; re-dispatch split (one file per call). `scripts/agent-watchdog.ps1` lists in-flight runs. Two consecutive escalations on one delegation → escalate to user.

### Hard prohibitions

- No `git` / `cargo` / `bun` / `just check` / `adb` / `tail` / `grep` from main thread. Poll `tmp/_pids.json` + log line-count, or dispatch `wt-investigate`.
- No Android JS change declared live without `[vite] hmr update` for the touched file. No Rust change declared live without fresh `connecting to 127.0.0.1:1420` after restart.

### Self-repair

Worker fails contract (missing artefact, no VERDICT, raw logs, gate bypass, scope creep) → re-delegate with sharper spec. Recurring → patch smallest closing rule in `.pi/agents/**` or `AGENTS.md`, finalise as `chore(agents): …`. Two failed repairs → escalate to user.

### Agent prompt quality bar

Enforced whenever `wt-edit` touches `.pi/agents/**` or this file:

- One job per agent, named in the first sentence of `description`.
- Frontmatter complete: `name`, `description`, `tools` (minimal), `systemPromptMode: replace`, `inheritProjectContext: true`, `inheritSkills: false`, `defaultContext: fresh`.
- Output contract appears before the work description.
- Every worker writes its `tmp/` artefact and returns a compact summary; missing artefact = no run.
- Body ≤ ~60 lines, imperative, no filler, no inline comments, no restating rules from this file.
