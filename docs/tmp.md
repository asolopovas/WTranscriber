# `tmp/` and `logs/` artefacts

Two separate scratch directories:

- **`logs/`** — per-tag build logs written by `scripts/run.ts` (one file
  per `--tag`, e.g. `logs/build.log`, `logs/deb.log`, `logs/and.log`).
  **Wiped at the start of every `just build*` recipe** (`build`,
  `build-host`, `build-deb-docker`). Safe to delete at any time.
- **`tmp/`** — dev-loop scratchpad. Every long-running session and every
  agent treats these files as the **source of truth** for liveness. Keep
  the table below in sync with reality; it is referenced by `AGENTS.md`,
  `docs/dev-loop.md`, `docs/android.md`, and the `wt-diagnose` /
  `wt-runner` agents.

## Inventory

| Path                          | Writer                                        | Reader                            | Lifetime                     |
| ----------------------------- | --------------------------------------------- | --------------------------------- | ---------------------------- |
| `tmp/_pids.json`              | `cargo xtask android bootstrap`               | `android-status`, agents, humans  | Live Android session         |
| `tmp/_platform`               | `android bootstrap`                           | `android-status`, `wt-diagnose`   | Live Android session         |
| `tmp/logcat.log`              | `adb logcat` (detached)                       | `wt-diagnose`, dev loop           | Live Android session         |
| `tmp/android-dev.log`         | `tauri android dev --no-watch`                | HMR liveness probe, `wt-diagnose` | Live Android session         |
| `tmp/android-dev.err.log`     | same                                          | `wt-diagnose` on failure          | Live Android session         |
| `tmp/logcat.err.log`          | `adb logcat` (detached)                       | `wt-diagnose` on failure          | Live Android session         |
| `tmp/dev-vital.{out,err}.log` | `scripts/dev-vital.ts` (spawned by bootstrap) | dev-loop heartbeat                | Live Android session         |
| `tmp/.bootstrap.stamp`        | `just bootstrap`                              | `just bootstrap-if-stale`         | Persistent until rebootstrap |
| `tmp/dev*.log`                | `just dev` (when redirected)                  | `wt-diagnose` desktop path        | Per dev session              |
| `tmp/diagnose-<slug>.md`      | `wt-diagnose` agent                           | Humans, follow-up agents          | Persistent (manual cleanup)  |
| `tmp/install-report.json`     | `wt-runner` (`install` mode)                  | Caller of `wt-runner`             | Overwritten per run          |
| `tmp/test-report.json`        | `wt-runner` (`test` mode)                     | Caller of `wt-runner`             | Overwritten per run          |

## Rules

- **Never `rm -rf tmp/` while a dev session is live** — kills the
  liveness contract. Use `just dev stop` first, then run the
  `clean-temp.ts` script.
- **`clean-temp.ts` is safe between turns**; the next bootstrap recreates
  everything it needs.
- **`tmp/_pids.json` exists ⇒ `:1420` belongs to Vite.** Do not run
  `just android-install`, `just android-build`, `cargo tauri build`, or
  any release build until `just dev stop` removes it.
- **`location.href` is not a liveness signal on Android.** Tauri reports
  `http://tauri.localhost/` even when HMR is dead. Always cross-check
  `tmp/logcat.log` for fresh `connecting to 127.0.0.1:1420`.

## Cleanup

```bash
bun scripts/clean-temp.ts             # remove tmp/ + agent session scratch (safe between turns)
bun scripts/clean-temp.ts --dry-run   # preview without deleting
bun scripts/clean-temp.ts --force     # ignore safety checks (rare)
just clean                            # full nuke: tmp/ + cargo target + node_modules + dist
```

`tmp/` is gitignored. Diagnose notes worth keeping should be moved
elsewhere (e.g. attach to a PR description) before running the script.
