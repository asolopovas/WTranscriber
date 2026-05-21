# AGENTS.md

Stack: Tauri 2 · Rust edition 2024 (MSRV 1.88, pinned via `rust-toolchain.toml`) · Vue 3 + TS + Vite · Bun · `just`.

This file is the agent table of contents, not the project manual. Keep durable knowledge in `docs/` and link it here. If a task uncovers a missing rule, stale workflow, or repeated mistake, update the relevant doc in the same change.

## Start here

| Need                                        | Source of truth                                                                          |
| ------------------------------------------- | ---------------------------------------------------------------------------------------- |
| Documentation map, ownership, freshness     | [`docs/README.md`](docs/README.md)                                                       |
| Repository layout, boundaries, IPC rules    | [`docs/architecture.md`](docs/architecture.md)                                           |
| Command execution and live dev workflow     | [`docs/dev-loop.md`](docs/dev-loop.md)                                                   |
| Verification matrix and pre-commit contract | [`docs/verification.md`](docs/verification.md)                                           |
| Android build/dev specifics                 | [`docs/android.md`](docs/android.md)                                                     |
| Release, signing, Windows VM                | [`docs/release.md`](docs/release.md)                                                     |
| Scratch artefacts and liveness files        | [`docs/tmp.md`](docs/tmp.md)                                                             |
| Build-speed constraints for native deps     | [`docs/rust-build-speed.md`](docs/rust-build-speed.md)                                   |
| ASR/transcription pipeline design           | [`docs/asr-pipeline-v2.md`](docs/asr-pipeline-v2.md)                                     |
| Current quality/debt ledger                 | [`docs/quality.md`](docs/quality.md), [`docs/technical-debt.md`](docs/technical-debt.md) |
| Multi-turn execution plans                  | [`docs/plans/README.md`](docs/plans/README.md)                                           |

## Commands

```bash
just dev               # desktop HMR; `just dev stop` to stop
just android           # Android USB/host HMR session (clean restart)
just check             # full local gate; accepts job tags
just check-changed     # changed-file gate for hooks/CI
just build             # Windows-only dev release matrix -> releases/dev/
just release           # publish dev; --stable bumps patch; --bump selects stable version
just bootstrap         # Windows host toolchain install + dependency prewarm
just setup             # bun install + git hooks
```

Run `just --list` for the complete command set. Prefer focused checks while iterating, then the verification matrix in [`docs/verification.md`](docs/verification.md) before handing off.

## Non-negotiable invariants

- `.githooks/pre-commit` is mandatory; `--no-verify` is forbidden except the release bump commit after `just check`.
- Vue/WebView owns presentation; Rust owns filesystem, models, native work, and long-running processes.
- `src/types.ts` mirrors Rust IPC structs.
- New Tauri command = `commands/<domain>.rs` handler + `lib.rs` `invoke_handler![…]` + `api.ts` wrapper + `types.ts` mirror + least-privilege capability permission when a plugin/API is touched.
- Large binary IPC payloads use raw bodies (`tauri::ipc::Request<'_>` / `Response`), not base64 strings.
- No comments in code. No `sleep` in scripts; poll with timeouts.
- Conventional commits, simple British English.

## Live dev quick rules

- Desktop liveness comes from the live `[dev]` stream and Vite on `http://localhost:1420/`.
- Android liveness comes from a fresh WebView `connecting to …:1420` line in `tmp/logcat.log`; `location.href` is not a signal.
- While `tmp/_pids.json` exists and Vite owns `:1420`, do not run APK/release/build commands that replace the debug-dev APK.
- JS/CSS edits should hot-reload; Rust/native/config/capability edits require restarting the Android bootstrap.

## Skills

Project-relevant skills live in `.agents/skills/` for pi and are mirrored under `.opencode/skills/` for opencode.

- `tauri-v2` — Tauri architecture, IPC, commands, capabilities, mobile, plugins, distribution.
- `tauri-debugging` — live desktop/Android/iOS WebView, CDP, logcat, Rust panic, IPC/capability triage.
- `rust-skills` — canonical Rust guidance.
- `chrome-devtools` — CDP/live browser inspection for the Vite/WebView surface.
- `playwright-skill` — browser automation and end-to-end UI probes.
- `error-resolver` — systematic diagnosis for errors, stack traces, and unexpected behaviour.
- `verification-loop` — post-change verification and pre-PR quality gates.
- `docker-patterns` — Docker release/build troubleshooting, especially Linux `.deb` packaging.
- `improve-codebase-architecture` — architectural refactoring and module-depth reviews.
