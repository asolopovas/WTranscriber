# Verification

Use the smallest check that proves the change while iterating. Before handoff, run the matrix row that matches the touched files. Command execution semantics and live-session liveness are owned by [`dev-loop.md`](dev-loop.md).

## Pre-commit hook

`.githooks/pre-commit` is mandatory; `--no-verify` is forbidden except for the release bump commit that runs after `just check`.

The hook:

1. Auto-formats touched Rust with `cargo fmt` for `src-tauri`/`xtask`.
2. Auto-formats touched TS/Vue/scripts/docs with `prettier --write`.
3. Re-stages formatted files.
4. Runs `bun scripts/check-changed.ts --staged`.

The hook covers changed-file formatting, TS/Vue typecheck/tests/lint, xtask clippy/tests, and dependency audits when lockfiles change. Compile-heavy native Rust correctness remains in explicit `just check` and release gates.

## Main gates

```bash
just check                 # full local gate; accepts job tags
just check typecheck js-test
just check-changed --staged
bun run typecheck
bun run lint-docs
bun run test
```

`just check` runs `cargo xtask check`, which fans out 11 jobs in parallel: `fmt-check`, `clippy`, `clippy-xtask`, `typecheck`, `vue-lint`, `knip`, `rust-test`, `xtask-test`, `js-test`, `machete`, `audit`. All jobs complete before the first failure is reported. The `fmt-check` job also runs `bun run lint-docs` so the documentation map, local links, and execution-plan structure stay mechanically enforced.

CI runs `just check-changed --base …`: formatting, lint, typecheck, tests, and audits are selected from changed files. Full native Rust/Tauri gates are local/release-only.

`just check` assumes C++ deps (`whisper-rs-sys`, `sherpa-onnx-sys`) are already built. `just setup` pre-warms them via `cargo build`. If `target/` is wiped, re-run `just setup` rather than letting `just check` pay the cold rebuild under parallel cargo lock contention.

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
- Android: diff `tmp/logcat.log` line counts. New failures require root cause from `logs/*.log`, `tmp/*.log`, `adb logcat`, and recent git history.
- Android JS edits must show `[vite] hmr update` in `tmp/android-dev.log`.
- New `am_kill`, `am_proc_died`, or `am_crash` for the app means inspect `tmp/logcat.log` around the timestamp before continuing.
