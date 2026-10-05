# Dev loop

## Run harness

Most `just` recipes use `scripts/run.ts`: `[tag]` output, `logs/<tag>.log`, default 90 s idle/600 s hard timeouts, exit 124 on timeout, and final `OK in X.Ys` or `FAIL exit=N in X.Ys`.

`just dev` disables both watchdogs. `just android` invokes xtask directly to survive quiet Cargo/Gradle phases. Checks are selected through [verification.md](verification.md).

## Desktop

`just dev` starts Vite and Tauri HMR; `just dev stop` stops desktop/Android sessions. Windows is the full release-matrix host. Linux supports desktop development, native installation, and the Docker/VM paths described in [release.md](release.md).

### Linux acceleration

`scripts/desktop.ts` selects CUDA for supported NVIDIA hardware/toolkits, checks compute architectures and the host compiler, and supplies pinned Sherpa/cuDNN runtime paths. Otherwise it uses `sherpa-static` CPU. `WT_CUDA=0` selects CPU; `WT_CUDA=1` requires CUDA.

Local builds target detected GPUs; override `CMAKE_CUDA_ARCHITECTURES` for other machines. An incompatible default compiler needs a supported g++ or `CMAKE_CUDA_HOST_COMPILER`. Installation copies selected GPU libraries beside the build and prioritises them over existing `LD_LIBRARY_PATH`; the staged CLI is probed before activation. Old Whisper ABI caches are rebuilt before development/installation.

## Android commands

| Task                                    | Command                                  |
| --------------------------------------- | ---------------------------------------- |
| Fresh USB HMR session                   | `just android`                           |
| LAN session                             | `just android host`                      |
| Select USB device                       | `just android usb <serial>`              |
| APK build/install without HMR           | `bun scripts/android-install.ts`         |
| Wipe/reinstall after signature mismatch | `bun scripts/android-install.ts --force` |
| Headless emulator                       | `bun scripts/android-emu.ts`             |

Bootstrap stops existing sessions and force-stops the app before restarting; it is not a no-op. USB uses `TAURI_DEV_HOST=127.0.0.1` and reverse forwards on 1420/1421. Host mode uses the detected LAN IP and `--host`. [android.md](android.md) owns prerequisites, bootstrap stages, and WebView inspection.

## Live-session signals

- Desktop: live `[dev]` output and Vite at `http://localhost:1420/`.
- Android startup: stage 6 attaches the WebView devtools socket; successful bootstrap prints `BOOTSTRAP OK`. `location.href` is not evidence of working HMR.
- Frontend changes: `[vite] hmr update /src/...` in `tmp/android-dev.log`.
- App crash/OOM: `am_crash`, `am_proc_died`, or `am_kill` in `tmp/logcat.log`.

While `tmp/_pids.json` exists and Vite owns 1420, APK/release builds can replace the debug app and strand HMR. Stop the session before `cargo xtask android build`, `android-install.ts`, `cargo tauri build`, or release commands. JS/CSS changes hot-reload; native/config/capability changes need `just dev stop` followed by `just android`.

Scratch file ownership and cleanup scope are in [tmp.md](tmp.md).
