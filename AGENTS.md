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
4. Spawn dev server + monitor as **detached Windows processes** (not `delegate async` — child agents propagate Ctrl-C on turn end and kill `just`/Vite/gradle with `STATUS_CONTROL_C_EXIT 0xC000013A`). Use PowerShell `Start-Process` so the child outlives the agent:
   ```bash
   powershell -Command "Start-Process -FilePath 'just' -ArgumentList 'android-dev' \
     -RedirectStandardOutput 'C:\Users\asolo\src\WTranscriber\tmp\android-dev.log' \
     -RedirectStandardError  'C:\Users\asolo\src\WTranscriber\tmp\android-dev.err.log' \
     -WorkingDirectory       'C:\Users\asolo\src\WTranscriber' \
     -WindowStyle Hidden -PassThru | Select-Object Id"
   ```
   Same pattern for `node scripts/error-monitor.mjs` → `tmp/error-monitor.log`. Reuse for `just dev` / `just android-dev-host`.
5. Record the PIDs (`tasklist //FI "PID eq <id>"` to confirm liveness; `taskkill //F //PID <id>` on shutdown).
6. **HMR sanity check** before declaring bootstrap done:
   - `curl -s http://localhost:9222/json` → ≥1 target whose URL is `http://tauri.localhost/` (USB) or `http://<LAN>:1420/` (Wi-Fi).
   - `tail -n 50 tmp/error-monitor.log` → no `WebSocket connection to 'ws://...:1421/' failed` lines in the last minute. If present, the WebView is on a stale HMR config; reload via CDP and re-check.
   - Touch a frontend file (`src/main.ts` mtime bump is enough) and confirm the device receives an HMR update via CDP console (`[vite] hot updated`). If silent, treat as a **bootstrap failure** — surface to the user, do not proceed.
7. Report bootstrap status to the user as a checklist.

### Per-turn protocol

Between every user turn — and after every edit batch — the orchestrator:

1. Reads `tmp/error-monitor.log` line count; compares against last seen.
2. **New lines → spawn `wt-triage`** with the _excerpt only_ (never the whole log):
   `"Error: <excerpt>. Diagnose and fix. Do not commit."`
   Triage returns `VERDICT / EVIDENCE / FIX`.
3. **Fix applied and green → spawn `wt-committer`** with a one-line change summary. Committer runs the pre-commit gate, writes a conventional message, pushes, returns the hash.
4. **Install / smoke needed → chain `install-and-test`** (`.pi/chains/`). Never call tester directly without installer.
5. **`just check` failure or regression → `wt-triage`** in parallel; it owns the multi-minute output.

### Decision table

| Signal                                            | Action                                                                                                 |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| New line in `tmp/error-monitor.log`               | `wt-triage` with excerpt                                                                               |
| Source edits applied, gate not yet hit            | `wt-committer` with summary                                                                            |
| Need a built artifact (Win GUI/CLI, Android, WSL) | `wt-installer`                                                                                         |
| Need 30s-clip smoke after install                 | chain `install-and-test`                                                                               |
| `just check` / CI / regression forensics          | `wt-triage` (parallel, observe-only)                                                                   |
| User asks to commit / ship                        | `wt-committer` (never `git commit` from main thread)                                                   |
| User asks to release                              | `wt-committer` → confirm clean → `just release-stable` brief delegated to `wt-installer` for artifacts |

### Self-repair (when an agent misbehaves)

Agents fail in two distinct ways. Treat them differently.

- **Code failure** (the work product is wrong: tests red, gate red, fix breaks build) → re-delegate to the same agent with a sharper task; the agent prompt is fine.
- **Agent failure** (the agent itself misbehaves: no `VERDICT/EVIDENCE/FIX` block, returns scratchpad, hangs, ignores file-signal contract, dumps raw logs, bypasses the gate, calls another agent, edits outside its scope, or repeatedly returns "completed without making edits" while the work was in fact done) → **repair the agent definition.**

#### Repair loop

1. **Detect.** Per-turn checks: did the worker return the contract block? Did the expected `tmp/*.json` / `tmp/*.md` artifact land? Did `git log` / build output reflect the claimed action? Two consecutive deviations from the same agent = repair trigger.
2. **Diagnose.** Read `.pi/agents/<name>.md`. Identify the missing or ambiguous instruction that allowed the deviation. Cross-check against the [agent instruction quality bar](#agent-instruction-quality-bar).
3. **Patch.** Edit `.pi/agents/<name>.md` with the **smallest** change that closes the gap — prefer tightening the output contract or adding one explicit prohibition over rewriting prose. Never grow the file just to add safety belts.
4. **Verify.** Re-run the same task. If the agent now conforms, route the patch through `wt-committer` as `chore(agents): tighten <name> <one-line reason>`. If it still deviates, escalate to the user with a one-paragraph diagnosis — do **not** loop more than twice.
5. **Record.** Every repair is its own commit so the history shows when and why an agent's prompt drifted.

For chains in `.pi/chains/`, the same loop applies; patch the chain file when the bug is in step wiring, not in a single agent.

### Hard prohibitions (orchestrator)

- No `git commit`, `git push`, `--no-verify`, `cargo`, `bun`, `just check`, or log grepping from the main thread.
- No agent-to-agent calls — workers signal via files under `tmp/`.
- No raw log dumps in responses to the user — relay `VERDICT / EVIDENCE / FIX` only.

## Subagents (`.pi/agents/`)

Orchestrator-worker pattern. Main thread = orchestrator (design + code + synthesis). Specialists run in fresh context and return tight summaries.

| Agent          | Role           | Trigger                                                                          |
| -------------- | -------------- | -------------------------------------------------------------------------------- |
| `wt-installer` | executor       | install/build artifact per platform (Win GUI + CLI, Android, WSL)                |
| `wt-tester`    | executor       | 30-second-clip smoke + assertion across platforms                                |
| `wt-committer` | gate-keeper    | stage, commit (pre-commit hook mandatory), push — **all** commits route here     |
| `wt-triage`    | diagnostician  | forensics on failing tests, CDP/logcat noise, `just check` failures, regressions |
| `wt-scout`     | reconnaissance | repo-wide code search; returns ranked `file:line` citations with annotations     |

Return contract for `wt-committer` and `wt-triage`: `VERDICT` / `EVIDENCE` / `FIX` block — no raw log dumps.

### Coordination rules

1. **No agent-to-agent calls.** Workers communicate only through the filesystem; the orchestrator is the only synthesizer.
2. **File-signal contract** (under `tmp/`): `install-report.json` (installer → tester), `test-report.json` (tester output), `error-monitor.log` (monitor → triage), `triage-<topic>.md` (triage artifacts). Workers never read each other's stdout.
3. **Chain when dependent, parallel when independent.** Installer → tester is a chain (tester reads installer's report). Triage runs in parallel with anything else — it only observes.
4. **Fresh context per worker** (`defaultContext: fresh`). The orchestrator carries project state; workers re-derive what they need.
5. **Orchestrator never** greps logs, runs `just check`, or commits directly. Delegate to `wt-triage` or `wt-committer`.
6. **Chains** live in `.pi/chains/`. Current: `install-and-test` (installer → tester).

### Agent instruction quality bar

Every `.pi/agents/*.md` file must hold this bar. The orchestrator enforces it during self-repair.

- **One job per agent.** The first sentence of the description names the single responsibility. If you cannot, the agent is misfactored — split it.
- **Frontmatter is load-bearing.** `name`, `description`, `tools`, `systemPromptMode: replace`, `inheritProjectContext: true`, `inheritSkills: false`, `defaultContext: fresh`. Tools list is minimal — grant nothing the job doesn't need.
- **Output contract first.** State the exact return shape (`VERDICT / EVIDENCE / FIX`, a JSON path, a commit hash) before describing the work. Workers regress to chatty prose without this anchor.
- **Inputs are files, not stdout.** Name the `tmp/*` artifacts the agent reads and writes. No "the previous agent told you" phrasing.
- **Prohibitions are explicit and short.** One bullet per prohibition (`Never bypass with --no-verify.`, `Never call another agent.`). Imperative, present tense, no hedging.
- **No project lore.** Reference `AGENTS.md` and `docs/` rather than restating rules — `inheritProjectContext: true` already pulls them in. Restating creates drift.
- **Compactness target.** Body under ~60 lines. If it grows, the agent is doing too much or repeating context.
- **Terse voice.** Skip preamble, no "Sure, I can help", no apologies, no meta-commentary on the task. Names carry intent.
- **No comments in code blocks** inside agent prompts — same rule as production code.

When editing an agent file: read it, change the smallest unique span, never reorder unrelated sections, run `bunx prettier --write` on the file, then route through `wt-committer`.
