# Release

## Commands

| Command                             | What it does                                                                     |
| ----------------------------------- | -------------------------------------------------------------------------------- |
| `just build`                        | Full dev matrix (Windows host + Linux `.deb` + Android APK) into `releases/dev/` |
| `just build-host`                   | Windows host installer only (no Docker)                                          |
| `just release`                      | Publish `releases/dev/*` to the rolling `dev` prerelease                         |
| `just release --stable`             | Stable release: check + bump patch + build + publish                             |
| `just release --bump [level]`       | Stable release with chosen bump; implies `--stable`                              |
| `cargo xtask bump [level]`          | Bump version, commit, tag (no push, no build)                                    |
| `cargo xtask release [--dev …]`     | Build artifacts into `releases/[dev/]`                                           |
| `cargo xtask publish <dev\|stable>` | Upload `releases/[dev/]*` to `dev` or `vX.Y.Z`                                   |

`level`: `patch` (default), `minor`, `major`, or explicit `X.Y.Z`.
`xtask release` flags (also accepted by `just release --stable`): `--dev`, `--no-host`, `--no-android`, `--no-deb`, `--no-windows-vm`, `--skip-rebuild`, `--sequential`.

## Prerequisites for a stable release

- gh CLI authenticated
- Branch not behind upstream
- `src-tauri/gen/android/keystore.properties` present (unless `--no-android`)
- Docker reachable (unless `--no-deb` and Android skipped/native)

## Docker (Windows host)

`just build` uses Docker Desktop's Linux engine for the `.deb` and (unless `WT_ANDROID_NATIVE=1`) the APK, via `asolopovas/wt-builder:debian12`. Start Docker first. On a `dockerDesktopLinuxEngine/_ping` 500 error, restart Docker/WSL. `WT_BUILDER_REBUILD=1` rebuilds the image; `WT_BUILDER_IMAGE=…` overrides the tag.

## Windows VM (Linux host)

`cargo xtask release` from Linux builds the NSIS installer over SSH against the VM under `windowsVm`. Set `sshHost` and `vmDir` in `release.config.local.json` (gitignored; falls back to committed `release.config.json`, or `WT_RELEASE_CONFIG`).

It auto-restarts the VM and retries the build once on failure. If `rust-std-x86_64-pc-windows-msvc is corrupt` persists, repair inside the VM: delete `~/.rustup` and `~/.cargo/bin`, reinstall via `rustup-init.exe`, re-add the msvc target.

## Channels

| Channel | Tag      | Filenames                                                      | Mutability |
| ------- | -------- | -------------------------------------------------------------- | ---------- |
| dev     | `dev`    | `wtranscriber-setup-<branch>.exe`, `wtranscriber-<branch>.apk` | rolling    |
| stable  | `vX.Y.Z` | `wtranscriber-setup-<ver>.exe`, `wtranscriber-<ver>.apk`       | versioned  |
| cuda    | `cuda`   | `wtranscriber-cuda-sm*-win-x64.zip`                            | rolling    |

Each release also ships `SHA256SUMS`, `release-manifest-<ver>.json`, and `<artifact>.sig` per binary when `TAURI_SIGNING_PRIVATE_KEY` is exported.

## Gates

- `xtask bump` / `publish stable`: clean tree; tag must (bump) / must not (publish) already exist
- `just release --stable`: runs the full local check first
- Stable release refuses an unsigned APK; `--dev` only warns
- `release-stable` without a bump may create the missing current tag but refuses to move an existing one

## Version sync (on bump)

Updates `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `workers/whisper-cuda-worker/Cargo.toml`, and both `Cargo.lock`s (via `cargo update -w --offline`). Do not hand-edit these versions.

## Android signing

Required for stable, recommended for dev. Create `src-tauri/gen/android/keystore.properties` with `storeFile`, `storePassword`, `keyAlias`, `keyPassword`. Signing is wired in automatically by `xtask/src/android/patch.rs`.

```
keytool -genkey -v -keystore ~/.keystores/wtranscriber-release.jks \
  -alias wtranscriber -keyalg RSA -keysize 4096 -validity 10000
```

Back up the keystore; losing it forfeits app identity.
