# AGENTS.md

Stack: Tauri 2 · Rust edition 2024 (MSRV 1.85) · Vue 3 + TS + Vite · Bun · `just` (wraps `cargo xtask`).

## Layout

```
src/                  Vue 3 frontend (api.ts, types.ts mirrors Rust)
src-tauri/src/        commands.rs, lib.rs (invoke_handler), bin/wt.rs (CLI), config/models/paths/error/transcriber/
xtask/src/            build / release / android orchestration
scripts/              cdp.mjs, error-monitor.mjs, diarize.py, install-*.ps1
docs/                 android · agents · dev-loop · release · rust-build-speed
.pi/agents/           coder · committer · installer · tester · triage · scout · researcher · docs-updater
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

Main thread coordinates only: design + delegate + synthesise. Never edits source, greps logs, runs `just check`, or commits.

### Bootstrap (per session)

1. `git config --get core.hooksPath` must be `.githooks`. Set if not. Ensure `tmp/` exists.
2. Ask platform if unknown (desktop / android USB / android Wi-Fi). Recipe is fixed per transport; switching mid-session strands HMR (reload via CDP first):
   - Desktop: `just dev`. Android USB: `just android-dev`. Android Wi-Fi: `just android-dev-host`.
   - Android: after WebView is up, `just android-debug-attach` forwards CDP `:9222`.
3. Spawn dev server + monitor as detached Windows processes (PowerShell `Start-Process`, see `docs/dev-loop.md`). Record PIDs.
4. HMR sanity check via `wt-triage`: CDP target URL correct, `tmp/error-monitor.log` clean of `:1421 failed`, touched `src/main.ts` triggers `[vite] hot updated`. Ports 1420/1421 must be free before relaunch.
5. Report bootstrap as a checklist.

Never instruct the user to run a dev command; the orchestrator launches it.

### Per-turn

Diff `tmp/error-monitor.log` line count after every user turn and edit batch. New lines → `wt-triage` with excerpt only. Then Decision table.

### Decision table

| Signal                                      | Action                                                                                                                             |
| ------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| New line in `tmp/error-monitor.log`         | `wt-triage` with excerpt                                                                                                           |
| Source edit needed                          | `wt-coder`                                                                                                                         |
| Edits applied, gate not hit                 | `wt-committer`                                                                                                                     |
| Need to find where X lives in repo          | `wt-scout`                                                                                                                         |
| Need a built artifact                       | `wt-installer`                                                                                                                     |
| Native edit during live dev                 | `just android-install` from main thread (never `wt-installer` / `just android-build`; both replace the debug-dev APK and kill HMR) |
| In-app misbehaviour or `just check` failure | `wt-triage`                                                                                                                        |
| 30 s-clip smoke after install               | chain `install-and-test`                                                                                                           |
| Commit / ship                               | `wt-committer`                                                                                                                     |
| Release                                     | `wt-committer` → `just release-stable` artifacts via `wt-installer`                                                                |
| External knowledge needed                   | `wt-researcher`                                                                                                                    |
| Recurring agent failure or workflow drift   | `wt-docs-updater` → `wt-committer` as `chore(agents): tighten <name>`                                                              |

### Coordination

- **Fresh context per worker** (`defaultContext: fresh`). Orchestrator carries project state; workers re-derive.
- **File-signal contract** under `tmp/`: every worker writes a JSON or `.md` artifact (`coder-report`, `last-commit`, `install-report`, `test-report`, `triage-<topic>`, `scout-<slug>`, `research-<slug>`, `docs-update`, `error-monitor.log`). Workers never read each other's stdout.
- **Chain when dependent, parallel when independent.** Chains live in `.pi/chains/`. Current: `install-and-test`.

### Hard prohibitions

- No `git`, `cargo`, `bun`, `just check`, or log probing (`tail`/`grep`/`adb`/`curl`/`tasklist`) from main thread.
- `wt-installer` is release-only; never during a live dev session.
- No agent-to-agent calls; signal via `tmp/` files.
- No raw log dumps to the user; relay `VERDICT / EVIDENCE / FIX` only.

### Self-repair

Tests red or build broken → re-delegate with a sharper spec. Agent misbehaves (missing contract block, raw-log dump, gate bypass, agent-to-agent call, scope creep) → `wt-docs-updater` patches the smallest closing rule. Two failed repair attempts → escalate to the user.

Agent quality bar: `.pi/agents/wt-docs-updater.md`.
