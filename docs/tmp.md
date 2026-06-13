# `tmp/` and `logs/` artefacts

Two scratch directories:

- **`logs/`** — per-tag run logs from `scripts/run.ts` (one file per `--tag`, e.g. `logs/build.log`). Wiped at the start of every `just build`. Safe to delete anytime.
- **`tmp/`** — dev-loop scratchpad and source of truth for live-session liveness. Keep the table below in sync; it is referenced by `AGENTS.md`, `docs/dev-loop.md`, and `docs/android.md`.

## Inventory

| Path                          | Writer                                        | Reader                 | Lifetime                  |
| ----------------------------- | --------------------------------------------- | ---------------------- | ------------------------- |
| `tmp/_pids.json`              | `cargo xtask android bootstrap`               | dev loop, humans       | Live Android session      |
| `tmp/_platform`               | `cargo xtask android bootstrap`               | dev loop               | Live Android session      |
| `tmp/logcat.log`              | `adb logcat` (detached)                       | HMR liveness, dev loop | Live Android session      |
| `tmp/logcat.err.log`          | `adb logcat` (detached)                       | dev loop on failure    | Live Android session      |
| `tmp/android-dev.log`         | detached Vite dev server                      | HMR liveness probe     | Live Android session      |
| `tmp/android-dev.err.log`     | same                                          | dev loop on failure    | Live Android session      |
| `tmp/android-tauri.log`       | `tauri android dev`                           | APK launch/build probe | Live Android session      |
| `tmp/android-tauri.err.log`   | same                                          | dev loop on failure    | Live Android session      |
| `tmp/dev-vital.{out,err}.log` | `scripts/dev-vital.ts` (spawned by bootstrap) | dev-loop heartbeat     | Live Android session      |
| `tmp/.setup.stamp`            | `just setup`                                  | `just setup-if-stale`  | Persistent until re-setup |

## Rules

- Never `rm -rf tmp/` while a dev session is live — it kills the liveness contract. Run `just dev stop`, then `bun scripts/clean-temp.ts`.
- `bun scripts/clean-temp.ts` is safe between turns; the next bootstrap recreates what it needs. It refuses to run while `tmp/_pids.json` lists a live pid (use `--force` for a stale file).
- `tmp/_pids.json` exists ⇒ `:1420` belongs to Vite. Do not run `cargo xtask android build`, `bun scripts/android-install.ts`, `cargo tauri build`, or any release build until `just dev stop` removes it.
- `location.href` is not a liveness signal on Android — Tauri reports `http://tauri.localhost/` even when HMR is dead. Use: `[vite] hmr update …` in `tmp/android-dev.log` (HMR), `am_crash`/`am_proc_died`/`am_kill` in `tmp/logcat.log` (crashes), and the bootstrap's `✓ WebView DevTools attached` / `BOOTSTRAP OK` (session up on `:1420`).

## Cleanup

```bash
bun scripts/clean-temp.ts             # remove tmp/ scratch (safe between turns)
bun scripts/clean-temp.ts --dry-run   # preview without deleting
bun scripts/clean-temp.ts --force     # ignore safety checks (rare)
```

`tmp/` is gitignored.
