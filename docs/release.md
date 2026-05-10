# Release

## Commands

| Command                       | What it does                                   |
| ----------------------------- | ---------------------------------------------- |
| `just release`                | Dev release; updates rolling `dev` prerelease  |
| `just release-stable [level]` | `check` + bump + tag + build + publish stable  |
| `just release-bump [level]`   | Bump version, commit, tag                      |
| `just release-build [--dev]`  | Build artifacts only into `releases/[dev/]`    |
| `just release-publish <ch>`   | Upload `releases/[dev/]*` to `dev` or `vX.Y.Z` |

`level`: `patch` (default), `minor`, `major`, or explicit `X.Y.Z`.
`release-build` flags: `--dev`, `--no-host`, `--no-android`, `--no-wsl`, `--no-windows-vm`, `--skip-rebuild`, `--sequential`.

## Channels

| Channel | Tag      | Filenames                                                      | Mutability |
| ------- | -------- | -------------------------------------------------------------- | ---------- |
| dev     | `dev`    | `wtranscriber-setup-<branch>.exe`, `wtranscriber-<branch>.apk` | rolling    |
| stable  | `vX.Y.Z` | `wtranscriber-setup-<ver>.exe`, `wtranscriber-<ver>.apk`       | immutable  |

## Artifacts

Bundle targets are pinned in `src-tauri/tauri.conf.json` (`bundle.targets = ["nsis", "deb", "app"]`):

- Windows host: `wtranscriber-setup-*.exe` (NSIS)
- Linux host: `wtranscriber_*_amd64.deb`
- macOS host (manual): `WTranscriber.app` bundle (not in the release matrix)
- Windows + WSL: `.deb` at `$HOME/.cache/wtranscriber-wsl-target/`
- Android: `wtranscriber-*.apk` (signed if `keystore.properties` present)
- `SHA256SUMS[-<ver>]` and `release-manifest-<ver>.json`
- `<artifact>.sig` per binary if `TAURI_SIGNING_PRIVATE_KEY` is exported

## Gates

| Stage             | Gate                                                                                                             |
| ----------------- | ---------------------------------------------------------------------------------------------------------------- |
| `release-bump`    | Working tree clean; tag does not exist                                                                           |
| `release-stable`  | `just check` (9 parallel jobs: fmt-check, clippy, typecheck, vue-lint, knip, rust-test, js-test, machete, audit) |
| `release-publish` | Stable: clean tree, local tag exists                                                                             |
| `release-build`   | Stable: refuses unsigned APK; dev: warns and continues                                                           |

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
