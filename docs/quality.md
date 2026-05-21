# Quality ledger

This ledger captures project quality in an agent-legible form. Update it when guardrails, architecture, or known risks change. Prefer turning repeated review feedback into checks.

## Current grades

| Area                    | Grade | Evidence                                                             | Gaps / next guardrail                                                                    |
| ----------------------- | ----- | -------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| Frontend TypeScript/Vue | B     | `vue-tsc`, Vitest, `scripts/lint-vue.ts`, aliases                    | More UI behaviour probes through CDP/Playwright for critical flows                       |
| Rust/Tauri IPC          | B     | Typed commands, `error::Error`, capability file, clippy/tests        | Add structural check that every command shape is mirrored in `src/types.ts`/`src/api.ts` |
| Transcription pipeline  | B     | Cache tests, ASR pipeline doc, engine boundaries                     | More fixture-based regression tests for diarization/cache/timestamp variants             |
| Android dev loop        | B+    | Bootstrap probes, logcat liveness, CDP forwarding, `docs/android.md` | Keep Tauri 2.11 workarounds isolated until removable                                     |
| Release tooling         | B     | `xtask release`, signing gates, Windows VM retry, manifests          | Add periodic dry-run/manifest validation if releases become frequent                     |
| Documentation           | B-    | `AGENTS.md` map, docs catalogue, plans/debt ledgers                  | Add doc link/freshness lint once doc volume grows                                        |

Grades are deliberately coarse: A = mechanically enforced and well tested; B = documented with meaningful checks; C = works but relies on memory/manual review; D = unclear or stale.

## Golden principles

- Boundaries over micromanagement: enforce module, IPC, and platform edges; allow local implementation freedom.
- Parse or type data at boundaries; do not build on guessed shapes.
- Prefer repository-local, inspectable knowledge over chat, memory, or external notes.
- Encode taste as tooling when possible: small checks with remediation-oriented errors beat long prose.
- Pay down drift continuously in small changes.

## Quality-gardening loop

Use this loop for recurring cleanup or after a confusing agent run:

1. Identify the repeated confusion, stale pattern, or escaped bug.
2. Decide whether it belongs in code, a test, a lint, a script, a skill, or docs.
3. Add the smallest enforceable guardrail.
4. Update this ledger if the grade or gap changes.
5. Move any resolved item out of [`technical-debt.md`](technical-debt.md).
