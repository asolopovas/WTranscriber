# AGENTS.md

Stack: Tauri 2 · Rust edition 2024 (MSRV 1.85) · Vue 3 + TS + Vite · Bun · `just` (wraps `cargo xtask`).

## Layout

```
src/                  Vue 3 frontend (api.ts wrappers, types.ts mirrors Rust)
src-tauri/src/        main.rs, bin/wt.rs (CLI), lib.rs (Builder + invoke_handler),
                      commands.rs (thin #[tauri::command]), config/models/paths/error,
                      transcriber/ (pipeline)
xtask/src/            build / release / android orchestration
scripts/              cdp.mjs, error-monitor.mjs, diarize.py, install-*.ps1
docs/                 android · agents · dev-loop · release · rust-build-speed
```

## Rules

- No comments in code. Names carry intent.
- No `sleep` in scripts. Wait on a real signal (process/file/log/polled condition + timeout).
- Edition 2024 (`LazyLock`, `let-else`, …). Errors crossing the JS boundary use `error::Error` (`Serialize`).
- `src/types.ts` mirrors Rust structs. TS/Vue imports use path aliases (`@/`, `@components/`, `@composables/`, `@utils/`, `@styles/`) — no `./` or `../`.
- Two-tier quality gate. Never bypass.
  - **Pre-commit** (`.githooks/pre-commit`): scoped to staged files — `cargo fmt --check` + `clippy -D warnings`, `prettier --check` + `vue-tsc`, doc prettier.
  - **Pre-release** (`just check`): full suite — fmt, clippy (pedantic + nursery, `-D warnings`), vue-tsc, vue lint, tests, `cargo machete`, `cargo audit` + `bun audit`. `just release-stable` chains it.

## Commands

```
just dev                desktop (HMR)
just android-dev[-host] Android USB / LAN (HMR)
just check              pre-release gate
just release-stable     check + bump + tag + build + publish
```

`just --list` for the rest. Domain details in `docs/`.

## Adding a Tauri command

1. Handler in `src-tauri/src/commands.rs`.
2. Register in `lib.rs` `invoke_handler![…]`.
3. Typed wrapper in `src/api.ts`; types in `src/types.ts`.

## Skills

- **`tauri-debugging`** (global) — canonical reference for Tauri 2 inspection on desktop / Android / iOS: build modes, WebView inspector, CDP, logcat tags, `tauri-plugin-log`, CrabNebula DevTools, IPC/capability errors, env vars, anti-patterns. Load before any debugging session.

## Workflows (see `docs/`)

- **Android + HMR**: `docs/android.md`. Key: `android-dev` is `--no-watch`, frontend-only; Rust rebuild = `just android-install` in a second terminal; ABI auto-detected (no `--target`).
- **Agent dev loop** (monitor + fixer + committer): `docs/agents.md`. Filesystem signaling via `tmp/error-monitor.log`; main thread never greps logs or runs `just check` — that's `wt-triage`'s job.
- **HMR + CDP + error monitor**: `docs/dev-loop.md`. Prefer CDP over screenshots for layout/style.
- **Release**: `docs/release.md`. **Build speed**: `docs/rust-build-speed.md`.

## Subagents (`.pi/agents/`)

Orchestrator-worker pattern. Main thread = orchestrator (design + code + synthesis). Specialists run in fresh context and return tight summaries.

| Agent          | Role          | Trigger                                                                          |
| -------------- | ------------- | -------------------------------------------------------------------------------- |
| `wt-installer` | executor      | install/build artifact per platform (Win GUI + CLI, Android, WSL)                |
| `wt-tester`    | executor      | 30-second-clip smoke + assertion across platforms                                |
| `wt-committer` | gate-keeper   | stage, commit (pre-commit hook mandatory), push — **all** commits route here     |
| `wt-triage`    | diagnostician | forensics on failing tests, CDP/logcat noise, `just check` failures, regressions |

Return contract for `wt-committer` and `wt-triage`: `VERDICT` / `EVIDENCE` / `FIX` block — no raw log dumps.

### Coordination rules

1. **No agent-to-agent calls.** Workers communicate only through the filesystem; the orchestrator is the only synthesizer.
2. **File-signal contract** (under `tmp/`): `install-report.json` (installer → tester), `test-report.json` (tester output), `error-monitor.log` (monitor → triage), `triage-<topic>.md` (triage artifacts). Workers never read each other's stdout.
3. **Chain when dependent, parallel when independent.** Installer → tester is a chain (tester reads installer's report). Triage runs in parallel with anything else — it only observes.
4. **Fresh context per worker** (`defaultContext: fresh`). The orchestrator carries project state; workers re-derive what they need.
5. **Orchestrator never** greps logs, runs `just check`, or commits directly. Delegate to `wt-triage` or `wt-committer`.
6. **Chains** live in `.pi/chains/`. Current: `install-and-test` (installer → tester).
