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

- Code-authoring rules live in `.pi/agents/wt-coder.md` (the only agent that edits source).
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

## Skills

- **`tauri-debugging`** (global) — canonical reference for Tauri 2 inspection on desktop / Android / iOS: build modes, WebView inspector, CDP, logcat tags, `tauri-plugin-log`, CrabNebula DevTools, IPC/capability errors, env vars, anti-patterns. Load before any debugging session.

## Workflows (see `docs/`)

- **Android + HMR** — `docs/android.md`.
- **Agent dev loop** — `docs/agents.md`.
- **HMR + CDP + error monitor** — `docs/dev-loop.md`.
- **Release** — `docs/release.md`. **Build speed** — `docs/rust-build-speed.md`.

## Orchestrator runbook

The main thread is the coordinator. It never edits without HMR, never greps logs, never runs `just check`, never commits directly. It executes this runbook autonomously — no user prompting required.

### Session bootstrap (run once per dev session)

1. Verify hook: `git config --get core.hooksPath` → must be `.githooks`. If not, set it.
2. Ensure `tmp/` exists.
3. Ask user platform if unknown: **desktop** or **android**. For android, also ask transport: **USB cable** or **Wi-Fi (no USB)**. If the user gives no transport, default to **USB**. Recipe follows transport and never changes mid-session:
   - Desktop: `just dev`.
   - Android USB: `just android-dev` (reverse-port over `tauri.localhost`).
   - Android Wi-Fi: `just android-dev-host` (HMR over `ws://<LAN>:1421`).
   - Android: after WebView is up, orchestrator runs `just android-debug-attach` to forward CDP `:9222`, then **verifies HMR is live** (next step). Switching transport after a session has begun strands the WebView on a stale HMR endpoint — the orchestrator must reload the page via CDP (`node scripts/cdp.mjs "location.reload()"`) before claiming bootstrap done.
   - **Never instruct the user to run a dev command.** Orchestrator launches it. Only fall back to asking if the spawn itself fails.
4. Spawn dev server + monitor as **detached Windows processes** — `delegate async` propagates Ctrl-C on turn end and kills `just`/Vite/gradle with `STATUS_CONTROL_C_EXIT 0xC000013A`. Use PowerShell `Start-Process` per `docs/dev-loop.md`. Record PIDs for shutdown.
5. **HMR sanity check** before declaring bootstrap done (delegate probes to `wt-triage` — never `curl`/`tail`/`adb` from main thread):
   - CDP target list shows `http://tauri.localhost/` (USB) or `http://<LAN>:1420/` (Wi-Fi).
   - No `ws://...:1421/ failed` in the last minute of `tmp/error-monitor.log` (stale HMR → reload via CDP).
   - Touch `src/main.ts`; confirm `[vite] hot updated` on device. Silent → **bootstrap failure**, surface to user.
   - Before relaunching `just android-dev` after a prior session: ports 1420/1421 must be free (orphaned Vite outlives a killed `just` parent).
6. Report bootstrap status to the user as a checklist.

### Per-turn protocol

After every user turn and every edit batch, diff `tmp/error-monitor.log` line count; new lines → `wt-triage` with the _excerpt only_. Then consult the Decision table.

### Decision table

| Signal                                                                | Action                                                                                                                                                |
| --------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| New line in `tmp/error-monitor.log`                                   | `wt-triage` with excerpt                                                                                                                              |
| Source edit needed (Rust / TS / Vue / xtask / gradle)                 | `wt-coder` (orchestrator never edits source from main thread)                                                                                         |
| Source edits applied, gate not yet hit                                | `wt-committer` with summary                                                                                                                           |
| Need a built artifact (Win GUI/CLI, Android, WSL)                     | `wt-installer`                                                                                                                                        |
| Native edit (Rust / kotlin / res / manifest / gradle) during live dev | `just android-install` from main thread — **never** `wt-installer` or `just android-build` (replaces debug-dev APK with bundled-asset APK, kills HMR) |
| User reports in-app misbehavior                                       | `wt-triage` with the symptom — never probe via `adb`/`curl`/`tail` directly                                                                           |
| Need 30s-clip smoke after install                                     | chain `install-and-test`                                                                                                                              |
| `just check` / CI / regression forensics                              | `wt-triage` (parallel, observe-only)                                                                                                                  |
| User asks to commit / ship                                            | `wt-committer` (never `git commit` from main thread)                                                                                                  |
| User asks to release                                                  | `wt-committer` → `just release-stable` artifacts via `wt-installer`                                                                                   |
| Need external knowledge (Reddit/GitHub/SO) before deciding            | `wt-researcher`                                                                                                                                       |

### Self-repair

Work-product wrong (tests red, build broken) → re-delegate with a sharper spec; the agent prompt is fine. **Agent misbehaves** (missing contract block, raw-log dump, gate bypass, agent-to-agent call, scope creep, or two consecutive deviations) → delegate to `wt-docs-updater` with the smallest closing change to `.pi/agents/<name>.md` (or `.pi/chains/*` if step wiring is at fault), then `wt-committer` as `chore(agents): tighten <name> <reason>`. After two failed repair attempts, escalate to the user.

### Hard prohibitions (orchestrator)

- No `git commit`, `git push`, `--no-verify`, `cargo`, `bun`, `just check`, or log probing (`tail`/`grep`/`adb logcat`/`adb shell`/`curl /json`/`tasklist`) from the main thread.
- `wt-installer` is release-only; never during a live `just dev`/`just android-dev` session.
- No agent-to-agent calls — workers signal via files under `tmp/`.
- No raw log dumps to the user — relay `VERDICT / EVIDENCE / FIX` only.

## Subagents (`.pi/agents/`)

Orchestrator-worker pattern. Main thread = orchestrator (design + code + synthesis). Specialists run in fresh context and return tight summaries.

| Agent           | Role           | Trigger                                                                                          |
| --------------- | -------------- | ------------------------------------------------------------------------------------------------ |
| `wt-installer`  | executor       | install/build artifact per platform (Win GUI + CLI, Android, WSL)                                |
| `wt-coder`      | executor       | apply orchestrator-specified code change to source files; run scoped checks; return diff summary |
| `wt-tester`     | executor       | 30-second-clip smoke + assertion across platforms                                                |
| `wt-committer`  | gate-keeper    | stage, commit (pre-commit hook mandatory), push — **all** commits route here                     |
| `wt-triage`     | diagnostician  | forensics on failing tests, CDP/logcat noise, `just check` failures, regressions                 |
| `wt-researcher` | external scout | external research / library or workflow questions / unfamiliar API / community-known gotchas     |
| `wt-scout`      | reconnaissance | repo-wide code search; returns ranked `file:line` citations with annotations                     |

Return contract for `wt-committer` and `wt-triage`: `VERDICT` / `EVIDENCE` / `FIX` block — no raw log dumps.

### Coordination rules

1. **File-signal contract** (under `tmp/`): `install-report.json` (installer → tester), `test-report.json` (tester output), `error-monitor.log` (monitor → triage), `triage-<topic>.md` (triage artifacts). Workers never read each other's stdout.
2. **Chain when dependent, parallel when independent.** Installer → tester is a chain. Triage runs in parallel — it only observes.
3. **Fresh context per worker** (`defaultContext: fresh`). The orchestrator carries project state; workers re-derive what they need.
4. **Chains** live in `.pi/chains/`. Current: `install-and-test` (installer → tester).

### Agent quality bar

See `.pi/agents/wt-docs-updater.md` — owned and enforced by the docs maintainer.
