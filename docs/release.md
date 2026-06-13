# Release

## Commands

| Command                                       | What it does                                                                     |
| --------------------------------------------- | -------------------------------------------------------------------------------- |
| `just build`                                  | Full dev matrix (Windows host + Linux `.deb` + Android APK) into `releases/dev/` |
| `just release`                                | Publish `releases/dev/*` to the rolling `dev` prerelease                         |
| `just release --stable`                       | Stable patch release: check + bump + build + publish                             |
| `just release --bump [level]`                 | Stable release with chosen bump; `--bump` implies `--stable`                     |
| `cargo xtask bump [level]`                    | Bump version, commit, tag (no push, no build)                                    |
| `cargo xtask release [--dev …]`               | Build artifacts into `releases/[dev/]`; use directly outside `just`              |
| `cargo xtask publish <dev\|stable>`           | Upload `releases/[dev/]*` to `dev` or `vX.Y.Z`                                   |
| `cargo xtask release-stable [--bump [level]]` | Lower-level stable flow; `just release --stable` is preferred                    |

`just release --stable` defaults to `--bump patch`. `level`: `patch` (default when `--bump` is present without a value), `minor`, `major`, or explicit `X.Y.Z`.
`xtask release` flags: `--dev`, `--no-host`, `--no-android`, `--no-deb`, `--no-windows-vm`, `--skip-rebuild`, `--sequential`. The same matrix flags work on `just release --stable` and are forwarded to `release-stable`.

`release-stable` preflights before any mutation (check, bump, tag): gh CLI authenticated, branch not behind upstream, `keystore.properties` present unless `--no-android`, Docker engine reachable unless `--no-deb` and Android is skipped or native. A failed preflight exits before the version bump, so a misconfigured host never leaves a half-released tag.

## Docker Desktop preflight on Windows

`just build` uses Docker Desktop's Linux engine for the Linux `.deb` and, unless `WT_ANDROID_NATIVE=1` is set, the Android APK. Start Docker Desktop before the full matrix and verify:

```powershell
docker info
docker run --rm hello-world
```

If Docker reports `dockerDesktopLinuxEngine/_ping` with `500 Internal Server Error`, restart Docker Desktop or WSL, then rerun `just build`. For a host-only installer while Docker is unavailable, use `just build-host`.

Docker-backed release steps use `asolopovas/wt-builder:debian12` by default and pull it from Docker Hub when it is not present locally. Set `WT_BUILDER_REBUILD=1` to rebuild `Dockerfile.builder` locally, or `WT_BUILDER_IMAGE=...` to use another image tag.

## Windows VM preflight

When `cargo xtask release` runs from Linux, it builds the Windows NSIS installer by SSH-driving the VM configured under `windowsVm`. Config resolution: `WT_RELEASE_CONFIG` if set, else `release.config.local.json` (gitignored, per-host), else the committed `release.config.json` example. Put your real `sshHost` alias and `vmDir` in `release.config.local.json`; it starts/restarts the VM dir through its Makefile.

```bash
make -C <vmDir> up
ssh <sshHost> '$PSVersionTable.PSVersion.ToString(); hostname'
```

`xtask release` runs the configured `startCommand` when SSH is down,
polls the configured `sshReadyCommand`, and falls back to the configured
`restartCommand` once before skipping the Windows `.exe`.

### Failsafe

`xtask release` is self-healing for transient failures: if the initial
SSH probe fails, or if the Windows build inside the VM exits non-zero,
the task runs the configured Windows VM restart command, polls SSH for up
to 5 min, and retries the build **once** before giving up. The build
script (`scripts/wt-windows-build.bat`) attempts a softer self-heal first —
`rustup component remove + self update + target add` — and as a last
resort `rustup toolchain uninstall + install`. This catches transient
rustup hiccups but **cannot** repair a persistent on-disk rustup state
corruption (e.g. a poisoned `~/.rustup/manifests` cache that survives
uninstall).

When `xtask release` keeps reporting `error: component manifest for
'rust-std-x86_64-pc-windows-msvc' is corrupt` after the auto-retry, run the
manual rustup repair once from inside the VM: delete `~/.rustup` and
`~/.cargo/bin` in the guest, reinstall via `rustup-init.exe`, and re-add the
msvc target. Then rebuild with `just build` on Windows or `cargo xtask release --dev`
on Linux, and publish with `just release`.

## Channels

| Channel | Tag      | Filenames                                                      | Mutability |
| ------- | -------- | -------------------------------------------------------------- | ---------- |
| dev     | `dev`    | `wtranscriber-setup-<branch>.exe`, `wtranscriber-<branch>.apk` | rolling    |
| stable  | `vX.Y.Z` | `wtranscriber-setup-<ver>.exe`, `wtranscriber-<ver>.apk`       | versioned  |
| cuda    | `cuda`   | `wtranscriber-cuda-sm*-win-x64.zip`                            | rolling    |

## Artifacts

Bundle targets are pinned in `src-tauri/tauri.conf.json` (`bundle.targets = ["nsis", "deb"]`). The release matrix adds Android through xtask:

- Windows host: `wtranscriber-setup-*.exe` (NSIS)
- Linux `.deb` (Docker, cross-platform): produced inside `asolopovas/wt-builder:debian12` by `cargo xtask release`, written to `src-tauri/target/release/bundle/deb/`
- Android: `wtranscriber-*.apk` (signed if `keystore.properties` present)
- `SHA256SUMS[-<ver>]` and `release-manifest-<ver>.json`
- `<artifact>.sig` per binary if `TAURI_SIGNING_PRIVATE_KEY` is exported

## Gates

| Stage                    | Gate                                                                                     |
| ------------------------ | ---------------------------------------------------------------------------------------- |
| `xtask bump`             | Working tree clean; tag does not exist                                                   |
| `just release --stable`  | `cargo xtask release-stable --bump patch` runs the full local check before build/publish |
| `xtask publish stable`   | Clean tree, local tag exists                                                             |
| `xtask release` (stable) | Refuses unsigned APK                                                                     |
| `xtask release --dev`    | Warns on unsigned APK and continues                                                      |

Stable releases use a new immutable `vX.Y.Z` tag. `just release --stable` bumps patch by default; `just release --bump minor`, `major`, or an explicit `X.Y.Z` chooses another version. Direct `cargo xtask release-stable` without a bump is only for first-publish or retry cases: it may create the current version tag when missing, but refuses to move an existing stable tag. Stable publish pushes the version tag and rolling `latest` tag, and marks the `vX.Y.Z` GitHub release as Latest. The bump commit uses `--no-verify` because `just check` already ran. The clean-tree gate runs **before** the version sync writes its files; otherwise the bump itself would dirty the tree.

## Version sync (on bump)

`package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `workers/whisper-cuda-worker/Cargo.toml`, `src-tauri/Cargo.lock` and `workers/whisper-cuda-worker/Cargo.lock` (refreshed via `cargo update -w --offline`).

## Android signing

Required for stable, recommended for dev.

```
src-tauri/gen/android/keystore.properties
  storeFile=
  storePassword=
  keyAlias=
  keyPassword=
```

`xtask/src/android/patch.rs` runs before each Android build. Idempotent; adds `signingConfigs.release` to the generated `app/build.gradle.kts`. Re-applies if Tauri regenerates. Falls back to `apksigner` if the keystore is outside the project.

Generate the keystore once (back it up; losing it forfeits app identity):

```
keytool -genkey -v -keystore ~/.keystores/wtranscriber-release.jks \
  -alias wtranscriber -keyalg RSA -keysize 4096 -validity 10000
```
