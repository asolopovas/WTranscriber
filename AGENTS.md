# AGENTS.md

Stack: Tauri 2 · Rust edition 2024 (MSRV 1.85) · Vue 3 + TS + Vite · Bun · `just` (wraps `cargo xtask`).

## Layout

```
src/                  Vue 3 frontend (api.ts, types.ts mirrors Rust)
src-tauri/src/        commands.rs, lib.rs (invoke_handler), bin/wt.rs (CLI), config/models/paths/error/transcriber/
xtask/src/            build / release / android orchestration
scripts/              cdp.mjs, error-monitor.mjs, dev-bootstrap.ps1, diarize.py, install-*.ps1
docs/                 android · agents · dev-loop · release · rust-build-speed
.pi/agents/           investigate · edit · runner · monitor
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

Four agents, one verb each, no scope overlap. Selection: _reading? mutating? touching a device? watching the swarm?_

| Agent            | Verb                  | Owns                                                                                                                                                                                                                                                                                                                                                   | Forbidden                                                                   |
| ---------------- | --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------- |
| `wt-investigate` | answer (read-only)    | `mode: map` locate, `mode: diagnose` root-cause one signal, `mode: research` web, `mode: review` diff                                                                                                                                                                                                                                                  | edits, builds, git, device installs                                         |
| `wt-edit`        | mutate files          | `mode: edit` (default): smallest spec-conforming edit + scoped checks. `mode: finalise` (explicit only): stage named paths, commit through pre-commit hook, push, write `tmp/last-commit.json`                                                                                                                                                         | `--no-verify`, `just check`, device installs                                |
| `wt-runner`      | install + test device | install + smoke-test on Win GUI, Win CLI, Android, WSL                                                                                                                                                                                                                                                                                                 | git, source edits, dev-session APK rebuild                                  |
| `wt-monitor`     | watch (read-only)     | concurrent snapshot of in-flight runs from named `tmp/` artefacts + async status files; flag stale / missing / contract drift; surface insights — bottlenecks, repeated contract misses, parallelism opportunities, verification gaps, handoff quality; recommend next orchestrator action + one process improvement; writes `tmp/monitor-<slug>.json` | edits, builds, tests, git, installs, managing / restarting / calling agents |

Sequencing: `wt-investigate` → `wt-edit` → optional `wt-runner` → `wt-edit mode: finalise` only when orchestrator names exact paths. Default `wt-edit` never touches git. `wt-monitor` runs concurrently with any of the above as a passive observer that reports both live coordination risks and workflow improvement insights — it observes artefacts and async status only, never substitutes for or manages another agent, never edits source, runs tests / builds, installs, commits, or calls agents. No agent-to-agent calls; agents communicate only through `tmp/` artefacts.

### Parallelism policy

Parallel is the default whenever artefact paths and source files do not overlap. Chain only when work is genuinely dependent.

- Parallel-safe: independent `wt-investigate` map / research / review tasks; multi-surface `wt-edit` runs whose file sets are disjoint (e.g. `src-tauri/` crate vs. `src/` view); `wt-runner mode: test` across distinct targets once each install is `pass`; `wt-monitor` may launch alongside any in-flight worker batch, or immediately after a batch completes to harvest workflow insights, since it only reads artefacts and writes `tmp/monitor-<slug>.json`.
- Sequential: `wt-investigate` → `wt-edit` on the same surface; `wt-runner mode: install` → `mode: test` for one target; `wt-edit mode: edit` → `wt-edit mode: finalise` on the named paths. Dependent work stays chained even when a `wt-monitor` snapshot runs alongside.
- Forbidden in parallel: any `wt-edit mode: finalise` running with another `wt-edit` or `wt-runner` (git history is single-writer; finalise is never parallel with edits or runners); two edits touching the same file or the same `tmp/` artefact; Android `wt-runner install` during a live HMR dev session (see live-dev invariant).

Skill `tauri-debugging` covers WebView inspector, CDP, logcat, IPC — load before debugging.

## Orchestrator runbook

Main thread: design + delegate + synthesise only. Never edits source, runs `git`/`cargo`/`bun`/`just check`/`adb`, or greps logs. State is re-derived each turn from `tmp/` artefacts and `git log`; never from prior agent stdout.

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

### Verification tiers

Three tiers, each owned by exactly one actor; never escalate without orchestrator instruction.

- **T1 scoped** (inside `wt-edit mode: edit`): format only touched files; `cargo check -p <crate>` if `.rs` touched; `vue-tsc --noEmit` if `.ts`/`.vue` touched; otherwise `skip`. Outcome recorded in `tmp/edit-report.json.checks`.
- **T2 gate** (`.githooks/pre-commit`, fires inside `wt-edit mode: finalise`): the only legitimate caller of `just check`-equivalent fmt/lint on staged lines. Never invoked manually from any agent or the main thread.
- **T3 device** (`wt-runner`): install + 30 s smoke on Win GUI / Win CLI / Android / WSL; the only actor allowed to touch installers, `adb`, or release binaries.

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
| Snapshot in-flight agent work       | `wt-monitor` with the run ids + `tmp/` artefact paths to inspect                                 |

### Artefact contract

All workers `defaultContext: fresh`; re-derive state from logs, `git log`, and the predecessor artefact named in the dispatch. Each worker reads exactly the artefact it depends on, writes its own, and returns only `VERDICT / EVIDENCE / FIX`. Missing artefact = no run; re-dispatch with the path spelled out. Single-owner artefacts:

| Artefact                    | Sole writer               | Typical reader                                             |
| --------------------------- | ------------------------- | ---------------------------------------------------------- |
| `tmp/investigate-<slug>.md` | `wt-investigate`          | orchestrator, then `wt-edit`                               |
| `tmp/edit-report.json`      | `wt-edit mode: edit`      | orchestrator, then `wt-edit mode: finalise` or `wt-runner` |
| `tmp/last-commit.json`      | `wt-edit mode: finalise`  | orchestrator                                               |
| `tmp/install-report.json`   | `wt-runner mode: install` | `wt-runner mode: test`                                     |
| `tmp/test-report.json`      | `wt-runner mode: test`    | orchestrator                                               |
| `tmp/monitor-<slug>.json`   | `wt-monitor`              | orchestrator                                               |

Workers never read each other's stdout or call each other. Orchestrator relays only `VERDICT / EVIDENCE / FIX`, never raw logs. Default to parallel dispatch when artefact paths and source files are disjoint (see parallelism policy); chain only when dependent. `wt-monitor` may read other workers' artefacts to report status but never writes outside `tmp/monitor-<slug>.json`.

### Watchdog

Default is async for any dispatch expected to exceed 60 s — `wt-edit` on more than one file, `wt-runner` of any mode, `wt-investigate mode: research`, or any `wt-investigate` over a multi-file diff. Sync dispatch is reserved for a single read-only investigate or a single-file edit. Async block:

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

Worker fails contract (missing artefact, no `VERDICT`, raw logs in chat, gate bypass, scope creep, finalise without named paths) → re-delegate once with a sharper spec naming the exact files and predecessor artefact. Recurring drift → patch the smallest closing rule in `.pi/agents/**` or this file, then finalise as `chore(agents): …`. Two failed repairs on one delegation → stop and escalate to user; never widen scope to compensate.

### Agent prompt quality bar

Enforced whenever `wt-edit` touches `.pi/agents/**` or this file:

- One job per agent, named in the first sentence of `description`.
- Frontmatter complete: `name`, `description`, `tools` (minimal), `systemPromptMode: replace`, `inheritProjectContext: true`, `inheritSkills: false`, `defaultContext: fresh`.
- Output contract appears before the work description and names the exact `tmp/` artefact path.
- Forbidden actions are listed explicitly in the description and again in stop rules.
- Every worker writes its `tmp/` artefact and returns only `VERDICT / EVIDENCE / FIX`; missing artefact = no run.
- Body ≤ ~60 lines, imperative, no filler, no inline comments, no restating rules from this file.
