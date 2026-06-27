# Release

## Commands

| Command                             | What it does                                                                     |
| ----------------------------------- | -------------------------------------------------------------------------------- |
| `just build`                        | Full dev matrix (Windows host + Linux `.deb` + Android APK) into `releases/dev/` |
| `just build-host`                   | Windows host installer only (no Docker)                                          |
| `just install [--interactive]`      | Build the host installer, then install it silently (`--interactive` for NSIS UI) |
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

`just build` uses Docker Desktop's Linux engine for the `.deb` and (unless `WT_ANDROID_NATIVE=1`) the APK, via `asolopovas/tauri-builder:debian12`. Start Docker first. On a `dockerDesktopLinuxEngine/_ping` 500 error, restart Docker/WSL. `WT_BUILDER_IMAGE=…` overrides the tag.

The Windows host installer builds **natively**, not in Docker — Tauri's NSIS bundling and WebView2 linking are unsupported on Linux.

### Builder image (reusable, public)

The builder is app-agnostic (Rust 1.88 + Bun + Tauri Linux deps + Android SDK/NDK + CUDA toolkit + cuDNN) and published to Docker Hub so contributors pull it instead of compiling the toolchain. `builders.rs` pulls it on demand. The image source lives in its own repo — [`asolopovas/tauri-app-container`](https://github.com/asolopovas/tauri-app-container) — where it is built and published (`just publish`); this repo only consumes the published tag. CUDA is included so CUDA-accelerated Linux builds work on NVIDIA hosts and so the image is reusable across other CUDA projects; `nvcc` is on `PATH` and the libs are on `LD_LIBRARY_PATH=/usr/local/cuda/lib64`.

Publish flow (in the [`tauri-app-container`](https://github.com/asolopovas/tauri-app-container) repo): `docker login` once, then `just publish`. A newly pushed Docker Hub repo is **public** by default. To flip an existing private repo public via API:

```bash
TOKEN=$(curl -s -H "Content-Type: application/json" \
  -d '{"username":"asolopovas","password":"<PAT>"}' \
  https://hub.docker.com/v2/users/login/ | jq -r .token)
curl -s -X PATCH -H "Authorization: JWT $TOKEN" -H "Content-Type: application/json" \
  -d '{"is_private":false}' \
  https://hub.docker.com/v2/repositories/asolopovas/tauri-builder/
```

CUDA is installed from NVIDIA's `debian12` apt repo (`cuda-minimal-build` + cuDNN 9) in a parallel `cuda` stage and copied into the final image, keeping the Debian 12 glibc baseline so the resulting `.deb` stays portable.

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
