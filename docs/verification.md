# Verification

Use the smallest check that proves the change while iterating. Before handoff, run the matrix row that matches the touched files. Execution semantics and live-session liveness are owned by [`dev-loop.md`](dev-loop.md).

## Pre-commit hook

`.githooks/pre-commit` is mandatory; `--no-verify` is forbidden except for the release bump commit after `just check`.

The hook:

1. Auto-formats touched Rust (`cargo fmt`, separate manifests for `src-tauri`/`xtask`).
2. Auto-formats touched TS/Vue/scripts/docs/config (`prettier --write`).
3. Re-stages formatted files.
4. Runs `bun scripts/check-changed.ts --staged`.

`check-changed.ts` selects gates from changed files: typecheck, Vue lint, doc lint, JS tests, xtask clippy/tests, and `cargo audit`/`bun audit` when lockfiles change. Compile-heavy native Rust correctness stays in `just check` / release gates.

## Main gates

```bash
just check                 # full local gate; accepts job tags
just check typecheck js-test
just check-changed --staged
bun run typecheck
bun run lint-docs
bun run test
```

`just check` runs `cargo xtask check`, fanning out 11 jobs in parallel: `fmt-check`, `clippy`, `clippy-xtask`, `typecheck`, `vue-lint`, `knip`, `rust-test`, `xtask-test`, `js-test`, `machete`, `audit`. All jobs run to completion before the first failure is reported. `fmt-check` also runs `bun run lint-docs`.

CI runs `just check-changed --base …`; full native Rust/Tauri gates are local/release-only.

`just check` assumes the C++ deps (`whisper-rs-sys`, `sherpa-onnx-sys`) are already built. `just setup` pre-warms them. If `target/` is wiped, re-run `just setup` rather than paying the cold rebuild under `just check`'s parallel cargo lock contention.

## Change-type matrix

| Change                        | Touch                                                                | Verify                                                        | Session action               |
| ----------------------------- | -------------------------------------------------------------------- | ------------------------------------------------------------- | ---------------------------- |
| Vue / TS / CSS                | `src/**`                                                             | `bun run typecheck`; UI/CDP probe when behaviour changes      | No restart; confirm HMR line |
| Rust command / IPC shape      | `commands/<domain>.rs`, `lib.rs`, `api.ts`, `types.ts`, `schemas.ts` | Focused Rust test/check + typecheck                           | Android: restart bootstrap   |
| Rust native / long-running    | `src-tauri/src/**`                                                   | Focused Rust test/check; inspect `RustStdoutStderr`           | Android: restart bootstrap   |
| Tauri config / capability     | `tauri.conf.json`, `capabilities/*.json`                             | Reproduce exact invoke; check IPC errors                      | Restart bootstrap            |
| Android scaffold / manifest   | `src-tauri/gen/android/**`                                           | `bun scripts/android-install.ts` + manual probe               | Restart bootstrap            |
| Release / build orchestration | `xtask/**`, `justfile`, `scripts/install-*`                          | Targeted command, then `just check`                           | Stop live dev first          |
| Docs only                     | `docs/**`, `AGENTS.md`                                               | `bun x prettier --check <changed docs>` + `bun run lint-docs` | No restart                   |

## Live-session review loop

- Desktop: scan the live `[dev]` stream for new error/panic lines.
- Android: diff `tmp/logcat.log` line counts; new failures need root cause from `logs/*.log`, `tmp/*.log`, `adb logcat`, and recent git history.
- Android JS edits must show `[vite] hmr update` in `tmp/android-dev.log`.
- New `am_kill`/`am_proc_died`/`am_crash` for the app means inspect `tmp/logcat.log` around the timestamp before continuing.
