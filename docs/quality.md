# Guardrail gaps

| Area          | Existing evidence                                 | Remaining gap                                                  |
| ------------- | ------------------------------------------------- | -------------------------------------------------------------- |
| Frontend      | Typecheck, Vitest, Vue lint, Zod IPC schemas      | More critical-flow UI probes                                   |
| IPC           | Typed commands/errors, capabilities, native tests | Structural TS/schema/Rust shape consistency check              |
| Transcription | Cache tests, queue tests, engine boundaries       | More real-audio timestamp/diarization fixtures                 |
| Android       | Bootstrap/socket/IPC probes and logcat            | Remove platform patches when upstream permits                  |
| Release       | Signing gates, manifests, VM retry                | Periodic dry-run/manifest checks if release frequency warrants |
| Documentation | Catalogue/link/plan lint                          | Freshness/ownership checks if the collection grows             |

Update gaps when checks or implementation change. Concrete compatibility/removal triggers live in [technical-debt.md](technical-debt.md); check selection lives in [verification.md](verification.md).
