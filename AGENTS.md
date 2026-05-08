# AGENTS.md

Stack: Tauri 2 · Rust edition 2024 (MSRV 1.85) · Vue 3 + TS + Vite · Bun · `just` (wraps `cargo xtask`).

## Layout

```
src/                  Vue 3 frontend (api.ts, types.ts mirrors Rust)
src-tauri/src/        commands.rs, lib.rs (invoke_handler), bin/wt.rs (CLI), config/models/paths/error/transcriber/
xtask/src/            build / release / android orchestration
scripts/              cdp.mjs, error-monitor.mjs, dev-bootstrap.ps1, diarize.py, install-*.ps1
docs/                 android · agents · dev-loop · release · rust-build-speed
.pi/agents/           investigate · edit · ship · runner
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

- Rust edition 2024 (`LazyLock`, `let-else`). Errors crossing the JS boundary use `error::Error` (`Serialize`).
- `src/types.ts` mirrors Rust structs.
- TS/Vue imports use path aliases (`@/`, `@components/`, `@composables/`, `@utils/`, `@styles/`).
- New Tauri command: `commands.rs` handler + `lib.rs` `invoke_handler![…]` + `api.ts` wrapper + `types.ts` mirror.
- No comments in code. No `sleep` in scripts; poll a real signal with timeout.
- Conventional commits, simple British English.

## Agent roster

Four agents, one verb each. Selection rule: _reading? writing files? writing git? touching a device?_

| Agent            | Verb                   | Use for                                                                                                                                             |
| ---------------- | ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `wt-investigate` | answer                 | find code (`mode: map`), diagnose one failing signal (`mode: diagnose`), research the web (`mode: research`), review a staged diff (`mode: review`) |
| `wt-edit`        | mutate                 | apply edits to source, docs, agent prompts, skills — anywhere under the repo                                                                        |
| `wt-ship`        | gate+commit            | stage, run pre-commit hook, write conventional message, push                                                                                        |
| `wt-runner`      | install/test on device | install + smoke-test on Win GUI, Win CLI, Android, WSL (mode-driven)                                                                                |

Skill `tauri-debugging` covers WebView inspector, CDP, logcat, IPC mechanics — load before any debugging session.

## Orchestrator runbook

Main thread coordinates only: design + delegate + synthesise. Never edits source, runs `git`/`cargo`/`bun`/`just check`, or greps logs directly.

> **Live dev invariant.** The WebView must serve live Vite assets throughout any dev session.
>
> - **Desktop**: WebView loads `http://localhost:1420/`. Confirm via `tmp/error-monitor.log` (no `:1421 failed`).
> - **Android**: `location.href` is always `http://tauri.localhost/` (Tauri custom scheme). The authoritative liveness signal is `tmp/android-dev.log` showing recent `connecting to 127.0.0.1:1420` from `RustStdoutStderr`. Absent → restart bootstrap.

### Bootstrap (per session)

Single command. The script handles hooks path, detached process spawning, port-owner tracking (not the `cmd /c` wrapper PID), `adb reverse`, logcat, and CDP forwarding:

```
powershell -ExecutionPolicy Bypass -File scripts/dev-bootstrap.ps1 -Platform <desktop|android-usb|android-host>
```

Exits non-zero if Vite never binds `:1420` or (on Android) the WebView never connects within 180 s. Writes `tmp/_platform`, `tmp/_pids.json`. After it returns OK, dispatch `wt-investigate` `mode: diagnose` on the latest log artefact to confirm a clean start before doing real work.

### Per-turn

- **Desktop**: line-count diff `tmp/error-monitor.log`. New lines → `wt-investigate` `mode: diagnose` with excerpt.
- **Android**: line-count diff `tmp/logcat.log`. New `am_kill`/`am_proc_died`/`am_crash` lines → `wt-investigate` with excerpt.
- After every edit batch on Android: JS edit must produce `[vite] hmr update` in `tmp/android-dev.log`; Rust/native edit requires restarting bootstrap (`just android-install` is forbidden — it ships a bundled-assets APK and strands HMR).

### Decision table

| Need                                               | Dispatch                                                                          |
| -------------------------------------------------- | --------------------------------------------------------------------------------- |
| Find/diagnose/research/review                      | `wt-investigate` (pick mode)                                                      |
| Edit any project file                              | `wt-edit`                                                                         |
| Commit + push                                      | `wt-ship`                                                                         |
| Build artefact, install, or smoke-test on a device | `wt-runner`                                                                       |
| 30 s smoke after install                           | chain `install-and-test`                                                          |
| Release                                            | `wt-ship` → `just release-stable` artefacts via `wt-runner`                       |
| Recurring agent/workflow drift                     | `wt-edit` on `.pi/agents/**` or `AGENTS.md`, then `wt-ship` as `chore(agents): …` |

### Coordination

- Fresh context per worker (`defaultContext: fresh`). Orchestrator carries project state; workers re-derive from logs and `git log`.
- File-signal contract under `tmp/`: every worker writes an artefact (`investigate-<slug>.md`, `edit-report.json`, `last-commit.json`, `install-report.json`, `test-report.json`). Workers never read each other's stdout, never call each other.
- Chain when dependent, parallel when independent. Current chain: `install-and-test`.

### Hard prohibitions

- No `git`/`cargo`/`bun`/`just check`/`adb`/`tail`/`grep` from the main thread. Poll `tmp/_pids.json` + `tmp/logcat.log` line-count, or dispatch `wt-investigate`.
- Never run `just android-install`, `just android-build`, or `wt-runner install` during a dev session — all replace the debug-dev APK and silently strand HMR. The only Android-side reinstall path during dev is re-running bootstrap.
- Do not declare an Android JS change live without `[vite] hmr update` for the touched file in `tmp/android-dev.log`. Do not declare a Rust change live without a fresh `connecting to 127.0.0.1:1420` after restart.
- No agent-to-agent calls. No raw log dumps to the user — relay `VERDICT / EVIDENCE / FIX` only.

### Self-repair

Worker fails contract (no VERDICT block, raw log dump, gate bypass, scope creep) → re-delegate with sharper spec. Recurring → `wt-edit` patches the smallest closing rule in `.pi/agents/**` or `AGENTS.md`, then `wt-ship`. Two failed repair attempts → escalate to user.

Agent quality bar (enforced when editing `.pi/agents/**`): one job per agent named in first sentence of `description`; frontmatter complete (`tools`, `systemPromptMode: replace`, `inheritProjectContext: true`, `inheritSkills: false`, `defaultContext: fresh`); output contract before description of work; body ≤ ~60 lines; imperative voice; no restating rules already in this file.
