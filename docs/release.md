# Release

## Commands

| Command                              | What it does                                                        |
| ------------------------------------ | ------------------------------------------------------------------- |
| `just build`                         | Windows-only shortcut: build full dev matrix into `releases/dev/`   |
| `just release`                       | Publish `releases/dev/*` to the rolling `dev` prerelease            |
| `just release-stable [level]`        | `check` + bump (commits + tags) + build + publish stable            |
| `cargo xtask bump [level]`           | Bump version, commit, tag (no push, no build)                       |
| `cargo xtask release [--dev …]`      | Build artifacts into `releases/[dev/]`; use directly outside `just` |
| `cargo xtask publish <dev\|stable>`  | Upload `releases/[dev/]*` to `dev` or `vX.Y.Z`                      |
| `cargo xtask release-stable [level]` | Local stable flow: check + bump + build + publish                   |

`level`: `patch` (default), `minor`, `major`, or explicit `X.Y.Z`.
`xtask release` flags: `--dev`, `--no-host`, `--no-android`, `--no-deb`, `--no-windows-vm`, `--skip-rebuild`, `--sequential`.

## Windows VM preflight

When `cargo xtask release` runs from Linux, it builds the Windows NSIS installer by SSH-driving the VM configured in `release.config.json` (`windowsVm`). The default config uses the `windows-vm` SSH alias and starts/restarts `/home/andrius/vms/win11` through its Makefile. Override the config path with `WT_RELEASE_CONFIG`.

```bash
make -C ~/vms/win11 up
ssh windows-vm '$PSVersionTable.PSVersion.ToString(); hostname'
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
'rust-std-x86_64-pc-windows-msvc' is corrupt` after the auto-retry,
run the manual nuke once from inside the VM:

```
powershell -ExecutionPolicy Bypass -File \\host.lan\Data\fix-rustup.ps1
```

Delete `~/.rustup` and `~/.cargo/bin` inside the guest, reinstall via `rustup-init.exe`, and re-add the msvc target. Then rebuild with `just build` on Windows or `cargo xtask release --dev` on Linux, and publish with `just release`.

## Channels

| Channel | Tag      | Filenames                                                      | Mutability |
| ------- | -------- | -------------------------------------------------------------- | ---------- |
| dev     | `dev`    | `wtranscriber-setup-<branch>.exe`, `wtranscriber-<branch>.apk` | rolling    |
| stable  | `vX.Y.Z` | `wtranscriber-setup-<ver>.exe`, `wtranscriber-<ver>.apk`       | immutable  |

## Artifacts

Bundle targets are pinned in `src-tauri/tauri.conf.json` (`bundle.targets = ["nsis", "deb"]`). The release matrix adds Android through xtask:

- Windows host: `wtranscriber-setup-*.exe` (NSIS)
- Linux `.deb` (Docker, cross-platform): produced inside `debian:12-slim` by `cargo xtask release`, written to `src-tauri/target/release/bundle/deb/`
- Android: `wtranscriber-*.apk` (signed if `keystore.properties` present)
- `SHA256SUMS[-<ver>]` and `release-manifest-<ver>.json`
- `<artifact>.sig` per binary if `TAURI_SIGNING_PRIVATE_KEY` is exported

## Gates

| Stage                    | Gate                                                                               |
| ------------------------ | ---------------------------------------------------------------------------------- |
| `xtask bump`             | Working tree clean; tag does not exist                                             |
| `just release-stable`    | `cargo xtask release-stable` runs the 11-job local check before bump/build/publish |
| `xtask publish stable`   | Clean tree, local tag exists                                                       |
| `xtask release` (stable) | Refuses unsigned APK                                                               |
| `xtask release --dev`    | Warns on unsigned APK and continues                                                |

The bump commit uses `--no-verify` because `just check` already ran. The clean-tree gate runs **before** the version sync writes its files; otherwise the bump itself would dirty the tree.

## Version sync (on bump)

`package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.lock` (refreshed via `cargo update -w --offline`).

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
