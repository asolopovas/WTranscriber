# Release

## Commands

| Command                             | What it does                                                                                     |
| ----------------------------------- | ------------------------------------------------------------------------------------------------ |
| `just build`                        | Full dev matrix (Windows host + Linux `.deb` + Android APK) into `releases/dev/`                 |
| `just build-host`                   | Windows host installer only (no Docker)                                                          |
| `just install [--interactive]`      | Bootstrap, build and install current checkout on Windows/Linux (`--interactive` is Windows-only) |
| `just release`                      | Publish `releases/dev/*` to the rolling `dev` prerelease                                         |
| `just release --stable`             | Stable release: check + bump patch + build + publish                                             |
| `just release --bump [level]`       | Stable release with chosen bump; implies `--stable`                                              |
| `cargo xtask bump [level]`          | Bump version, commit, tag (no push, no build)                                                    |
| `cargo xtask release [--dev …]`     | Build artifacts into `releases/[dev/]`                                                           |
| `cargo xtask publish <dev\|stable>` | Upload `releases/[dev/]*` to `dev` or `vX.Y.Z`                                                   |

`level`: `patch` (default), `minor`, `major`, or explicit `X.Y.Z`.
`xtask release` flags (also accepted by `just release --stable`): `--dev`, `--no-host`, `--no-android`, `--no-deb`, `--no-windows-vm`, `--skip-rebuild`, `--sequential`.

## Installing a branch locally

Run `just install` from the checkout to build its current contents, including uncommitted changes. Pull updates yourself before running it when you want newer commits; installation does not change branches or fetch code. Internet access is required on a fresh machine.

- Windows bootstraps missing tools with `scripts/bootstrap-windows.ps1`, refreshes the environment in the same invocation, then builds and installs the x64 NSIS bundle for the current checkout. It selects the current version from `src-tauri/target/release/bundle/nsis`, independently of release-channel artifacts or branch naming. Windows App Installer (`winget`) and administrator access are needed for toolchain setup. `--interactive` enables the installer UI.
- Linux bootstraps Bun/Rust and native prerequisites using apt, dnf, pacman or zypper. Other distributions must provide native prerequisites themselves. Builds use CPU or supported CUDA as described in [Linux acceleration](dev-loop.md#linux-acceleration); Docker is not required. Binaries and launchers go under `${WT_INSTALL_PREFIX:-$HOME/.local}`; desktop integration uses `${XDG_DATA_HOME:-$HOME/.local/share}`. Only missing system prerequisites require sudo; compilation and app installation run as the calling user.
- Linux installation stages both binaries and native shared libraries, probes `wt --help`, then switches the installed build. A failed build or CLI probe keeps the previous build. A lock prevents concurrent installation into the same prefix.
- macOS and other desktop systems are rejected before setup because the app's native runtime downloads do not support them.

`just install --help` and invalid-option checks run without installing dependencies. A recorded dev session must be stopped with `just stop` before installation.

## Prerequisites for a stable release

- gh CLI authenticated
- Branch not behind upstream
- `src-tauri/gen/android/keystore.properties` present (unless `--no-android`)
- Docker reachable (unless `--no-deb` and Android skipped/native)

## Docker (Windows host)

`just build` uses Docker Desktop's Linux engine for the `.deb` and (unless `WT_ANDROID_NATIVE=1`) the APK, via `asolopovas/tauri-builder:debian12`. Start Docker first. On a `dockerDesktopLinuxEngine/_ping` 500 error, restart Docker/WSL. `WT_BUILDER_IMAGE=…` overrides the tag.

The Windows host installer builds **natively**, not in Docker — Tauri's NSIS bundling and WebView2 linking are unsupported on Linux.

### Builder image

`xtask/src/release/builders.rs` pulls `asolopovas/tauri-builder:debian12` on demand; `WT_BUILDER_IMAGE` overrides it. Image source/publication belongs in [tauri-app-container](https://github.com/asolopovas/tauri-app-container). It supplies Linux/Android toolchains and CUDA while retaining a Debian 12 glibc baseline.

## Windows VM (Linux host)

`cargo xtask release` from Linux builds the NSIS installer over SSH against the VM under `windowsVm`. Set `sshHost` and `vmDir` in `release.config.local.json` (gitignored; falls back to committed `release.config.json`, or `WT_RELEASE_CONFIG`).

It restarts the VM and retries once on failure. Persistent toolchain corruption must be diagnosed and repaired inside that VM before retrying.

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

Keep the release keystore: replacements cannot update installations signed by the old identity.
