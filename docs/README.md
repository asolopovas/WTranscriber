# Documentation map

`AGENTS.md` is the table of contents; durable detail lives here so agents load only what a task needs.

## Catalogue

| Doc                                          | Purpose                                                       | Verification status                                                                      |
| -------------------------------------------- | ------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| [`architecture.md`](architecture.md)         | Layout, layering, IPC boundaries, project conventions         | Should match tree and Tauri command wiring                                               |
| [`dev-loop.md`](dev-loop.md)                 | Local command contract, desktop/Android HMR, liveness signals | Backed by `justfile`, `scripts/run.ts`, `xtask` bootstrap                                |
| [`verification.md`](verification.md)         | Pre-commit, checks, change-type verification matrix           | Backed by `.githooks/pre-commit`, `scripts/check-changed.ts`, `cargo xtask check`        |
| [`android.md`](android.md)                   | Android prerequisites, build/install, bootstrap guarantees    | Backed by `xtask/src/android/**`, Android generated project                              |
| [`release.md`](release.md)                   | Release commands, artefacts, signing, Windows VM path         | Backed by `xtask/src/release/**`, `release.config.json`                                  |
| [`tmp.md`](tmp.md)                           | `tmp/` and `logs/` artefact inventory                         | Backed by `scripts/run.ts`, android bootstrap, cleanup script                            |
| [`rust-build-speed.md`](rust-build-speed.md) | Native dependency cache and build-speed guidance              | Backed by `src-tauri/build.rs`, `xtask/src/check.rs`                                     |
| [`asr-pipeline.md`](asr-pipeline.md)         | ASR/transcription pipeline design                             | Backed by `src-tauri/src/transcriber/**`, `engine/**`, `diarizer/**`                     |
| [`quality.md`](quality.md)                   | Current quality grades and guardrail gaps                     | Updated when architecture or checks change; catalogue enforced by `scripts/lint-docs.ts` |
| [`technical-debt.md`](technical-debt.md)     | Known debt, temporary patches, cleanup triggers               | Updated when debt is added or retired; local links enforced by `scripts/lint-docs.ts`    |
| [`plans/README.md`](plans/README.md)         | Execution-plan lifecycle and directory contract               | Plan directories and required plan headings are enforced by `scripts/lint-docs.ts`       |

## Rules

- Keep each doc small enough to read in one pass; split by domain when a file becomes a grab bag.
- Every doc names the code or command that verifies it. `bun run lint-docs` enforces catalogue links, local Markdown links, `AGENTS.md` size, and execution-plan shape.
- When a rule becomes mechanical, encode it in tooling rather than prose. When an agent gets confused twice, promote the missing context into `docs/` or a skill.

## Update checklist

When changing behaviour:

1. Does `AGENTS.md` still point to the right source of truth?
2. Does the relevant doc describe the new workflow or invariant?
3. Can a check, test, lint, or script enforce the rule instead?
4. Multi-turn work → add an execution plan under `docs/plans/active/`.
5. Did debt move between `technical-debt.md`, `quality.md`, and completed work?
