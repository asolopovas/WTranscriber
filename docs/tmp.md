# `tmp/` artefacts

`tmp/` is the dev-loop scratchpad. Every long-running session and every
agent treats these files as the **source of truth** for liveness. Keep
this table in sync with reality; it is referenced by `AGENTS.md`,
`docs/dev-loop.md`, `docs/android.md`, and the `wt-diagnose` / `wt-runner`
agents.

## Inventory

| Path                      | Writer                          | Reader                            | Lifetime                    |
| ------------------------- | ------------------------------- | --------------------------------- | --------------------------- |
| `tmp/_pids.json`          | `cargo xtask android bootstrap` | `android-status`, agents, humans  | Live Android session        |
| `tmp/_platform`           | `android bootstrap`             | `android-status`, `wt-diagnose`   | Live Android session        |
| `tmp/logcat.log`          | `adb logcat` (detached)         | `wt-diagnose`, dev loop           | Live Android session        |
| `tmp/android-dev.log`     | `tauri android dev --no-watch`  | HMR liveness probe, `wt-diagnose` | Live Android session        |
| `tmp/android-dev.err.log` | same                            | `wt-diagnose` on failure          | Live Android session        |
| `tmp/dev*.log`            | `just dev` (when redirected)    | `wt-diagnose` desktop path        | Per dev session             |
| `tmp/diagnose-<slug>.md`  | `wt-diagnose` agent             | Humans, follow-up agents          | Persistent (manual cleanup) |
| `tmp/install-report.json` | `wt-runner` (`install` mode)    | Caller of `wt-runner`             | Overwritten per run         |
| `tmp/test-report.json`    | `wt-runner` (`test` mode)       | Caller of `wt-runner`             | Overwritten per run         |

## Rules

- **Never `rm -rf tmp/` while a dev session is live** — kills the
  liveness contract. Use `just android-stop` first, then `just clean-temp`.
- **`clean-temp` is safe between turns**; the next bootstrap recreates
  everything it needs.
- **`tmp/_pids.json` exists ⇒ `:1420` belongs to Vite.** Do not run
  `just android-install`, `just android-build`, `cargo tauri build`, or
  any release build until `android-stop` removes it.
- **`location.href` is not a liveness signal on Android.** Tauri reports
  `http://tauri.localhost/` even when HMR is dead. Always cross-check
  `tmp/logcat.log` for fresh `connecting to 127.0.0.1:1420`.

## Cleanup

```bash
just clean-temp             # remove tmp/ + agent session scratch (safe between turns)
just clean-temp --dry-run   # preview without deleting
just clean-temp --force     # ignore safety checks (rare)
```

`tmp/` is gitignored. Diagnose notes worth keeping should be moved
elsewhere (e.g. attach to a PR description) before running `clean-temp`.
