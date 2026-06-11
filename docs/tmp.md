# `tmp/` and `logs/` artefacts

Two separate scratch directories:

- **`logs/`** — per-tag build logs written by `scripts/run.ts` (one file
  per `--tag`, e.g. `logs/build.log`, `logs/deb.log`, `logs/and.log`).
  **Wiped at the start of every `just build`.** Safe to delete at any time.
- **`tmp/`** — dev-loop scratchpad. Every long-running session treats
  these files as the **source of truth** for liveness. Keep the table
  below in sync with reality; it is referenced by `AGENTS.md`,
  `docs/dev-loop.md`, and `docs/android.md`.

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
| `tmp/dev*.log`                | `just dev` (when redirected)                  | dev-loop desktop path  | Per dev session           |

## Rules

- **Never `rm -rf tmp/` while a dev session is live** — kills the
  liveness contract. Use `just dev stop` first, then
  `bun scripts/clean-temp.ts`.
- **`bun scripts/clean-temp.ts` is safe between turns**; the next
  bootstrap recreates everything it needs.
- **`tmp/_pids.json` exists ⇒ `:1420` belongs to Vite.** Do not run
  `cargo xtask android build`, `bun scripts/android-install.ts`,
  `cargo tauri build`, or any release build until `just dev stop`
  removes it.
- **`location.href` is not a liveness signal on Android.** Tauri reports
  `http://tauri.localhost/` even when HMR is dead. Always cross-check
  `tmp/logcat.log` for a fresh WebView connection to `:1420` (`127.0.0.1`
  for localhost/emulator flows, or the host LAN IP chosen by Tauri on
  physical devices).

## Cleanup

```bash
bun scripts/clean-temp.ts             # remove tmp/ scratch (safe between turns)
bun scripts/clean-temp.ts --dry-run   # preview without deleting
bun scripts/clean-temp.ts --force     # ignore safety checks (rare)
```

`tmp/` is gitignored.
