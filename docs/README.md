# Documentation map

The repository is the system of record for agent-operable knowledge. Keep `AGENTS.md` short and put durable details here so agents can progressively disclose context instead of loading a monolithic manual.

## Catalogue

| Doc                                          | Purpose                                                       | Verification status                                                               |
| -------------------------------------------- | ------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| [`architecture.md`](architecture.md)         | Layout, layering, IPC boundaries, project conventions         | Should match tree and Tauri command wiring                                        |
| [`dev-loop.md`](dev-loop.md)                 | Local command contract, desktop/Android HMR, liveness signals | Backed by `justfile`, `scripts/run.ts`, `xtask` bootstrap                         |
| [`verification.md`](verification.md)         | Pre-commit, checks, change-type verification matrix           | Backed by `.githooks/pre-commit`, `scripts/check-changed.ts`, `cargo xtask check` |
| [`android.md`](android.md)                   | Android prerequisites, build/install, bootstrap guarantees    | Backed by `xtask/src/android/**`, Android generated project                       |
| [`release.md`](release.md)                   | Release commands, artefacts, signing, Windows VM path         | Backed by `xtask/src/release/**`, `release.config.json`                           |
| [`tmp.md`](tmp.md)                           | `tmp/` and `logs/` artefact inventory                         | Backed by `scripts/run.ts`, android bootstrap, cleanup script                     |
| [`rust-build-speed.md`](rust-build-speed.md) | Native dependency cache and build-speed guidance              | Backed by `src-tauri/build.rs`, `xtask/src/check.rs`                              |
| [`asr-pipeline-v2.md`](asr-pipeline-v2.md)   | ASR/transcription pipeline design                             | Backed by `src-tauri/src/transcriber/**`, `engine/**`, `diarizer/**`              |
| [`quality.md`](quality.md)                   | Current quality grades and guardrail gaps                     | Update when architecture or checks change                                         |
| [`technical-debt.md`](technical-debt.md)     | Known debt, temporary patches, cleanup triggers               | Update when debt is added or retired                                              |
| [`plans/README.md`](plans/README.md)         | Execution-plan lifecycle and directory contract               | Use for multi-turn work                                                           |

## Agent-first documentation rules

- Prefer a map plus links over a large instruction blob.
- Store decisions, workflows, quality expectations, and temporary patches in versioned Markdown near the code they explain.
- Every doc should say what code or command verifies it.
- When a rule becomes repeatable and mechanical, encode it in tooling rather than prose.
- When an agent gets confused twice, promote the missing context into `docs/` or a project skill.
- Keep docs small enough to read in one pass; split by domain when a file becomes a grab bag.

## Update checklist

When changing behaviour, ask:

1. Does `AGENTS.md` still point to the right source of truth?
2. Does the relevant doc describe the new workflow or invariant?
3. Is there a check, test, lint, or script that can enforce the rule?
4. If the work spans multiple turns, should there be an execution plan under `docs/plans/active/`?
5. Did any known debt move between `technical-debt.md`, `quality.md`, and completed work?
