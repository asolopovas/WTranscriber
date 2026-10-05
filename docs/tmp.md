# Scratch files

`logs/<tag>.log` belongs to `scripts/run.ts` and is reset by `just build`. `tmp/` contains dev-session state; do not delete it during a live session. Liveness/restart rules are in [dev-loop.md](dev-loop.md#live-session-signals).

| Path                              | Writer / purpose                             |
| --------------------------------- | -------------------------------------------- |
| `tmp/_pids.json`, `_platform`     | Android bootstrap: live processes/platform   |
| `tmp/logcat.{log,err.log}`        | adb: crash/process/native diagnostics        |
| `tmp/android-dev.{log,err.log}`   | Bootstrap-owned Vite: HMR/server diagnostics |
| `tmp/android-tauri.{log,err.log}` | Tauri Android: build/launch diagnostics      |
| `tmp/dev-vital.{out,err}.log`     | `scripts/dev-vital.ts` heartbeat             |
| `tmp/.setup.stamp`                | Setup freshness marker                       |

All are gitignored; bootstrap recreates session files.

## Cleanup scope

`bun scripts/clean-temp.ts --dry-run` previews its targets. The script removes project `tmp/`, `.playwright-cli/`, project-specific Pi/Claude sessions, and related Claude file-history/session-env/tasks. It is broader than scratch-file cleanup.

Normal cleanup refuses live PIDs in `_pids.json`; stop with `just dev stop` first. `--force` bypasses that check and does not establish that a session is stale. Preserve wanted session/history evidence before an intentional cleanup.

Source: `scripts/clean-temp.ts`.
