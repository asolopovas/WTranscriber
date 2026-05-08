# Release

Module of [`AGENTS.md`](../AGENTS.md). Covers release channels, gates, signing, and build-speed reference numbers. Compile-time tuning lives in [`rust-build-speed.md`](rust-build-speed.md).

## Commands

| Command                       | What it does                                            |
| ----------------------------- | ------------------------------------------------------- |
| `just release`                | Dev release; updates the rolling `dev` prerelease       |
| `just release-stable [level]` | `check` + bump + tag + build + publish stable           |
| `just release-bump [level]`   | Bump version, commit, tag                               |
| `just release-build [--dev]`  | Build artifacts only into `releases/[dev/]`             |
| `just release-publish <ch>`   | Upload existing `releases/[dev/]*` to `dev` or `vX.Y.Z` |

`level`: `patch` (default), `minor`, `major`, or explicit `X.Y.Z`.

`release-build` flags: `--no-host`, `--no-android`, `--no-wsl`, `--skip-rebuild`, `--sequential`.

## Channels

| Channel | Tag      | Filenames                                                      | Mutability |
| ------- | -------- | -------------------------------------------------------------- | ---------- |
| dev     | `dev`    | `wtranscriber-setup-<branch>.exe`, `wtranscriber-<branch>.apk` | rolling    |
| stable  | `vX.Y.Z` | `wtranscriber-setup-<ver>.exe`, `wtranscriber-<ver>.apk`       | immutable  |

Stable filenames carry the version (stable URLs per release). Dev filenames carry the branch (URLs stay constant across builds).

## Artifacts per release

- Windows host: `wtranscriber-setup-*.exe` (NSIS)
- Linux host: `wtranscriber_*_amd64.deb`
- Windows + WSL: `.deb` built in WSL at `$HOME/.cache/wtranscriber-wsl-target/`
- Android: `wtranscriber-*.apk` (signed if `keystore.properties` present)
- `SHA256SUMS[-<ver>]` and `release-manifest-<ver>.json`
- `<artifact>.sig` per binary if `TAURI_SIGNING_PRIVATE_KEY` is exported

## Gates

| Stage             | Gate                                                   |
| ----------------- | ------------------------------------------------------ |
| `release-bump`    | Working tree clean; tag `vX.Y.Z` does not exist        |
| `release-stable`  | `just check` (fmt + clippy + typecheck + test)         |
| `release-publish` | Stable: clean tree, local tag exists                   |
| `release-build`   | Stable: refuses unsigned APK; dev: warns and continues |

The bump commit uses `--no-verify` because `just check` already ran.

## Version sync (on bump)

- `package.json`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`
- `src-tauri/Cargo.lock` (refreshed via `cargo update -w --offline`)

## Android signing

Required for stable, recommended for dev.

```
src-tauri/gen/android/keystore.properties     (gitignored; see .example)
  storeFile=
  storePassword=
  keyAlias=
  keyPassword=
```

`scripts/patch-android-signing.mjs` runs before each Android build. Idempotent; adds a `signingConfigs.release` block to the generated `app/build.gradle.kts`. Re-applies if Tauri regenerates the file. When the keystore file is not in the project, the script also calls `apksigner` directly.

Generate the keystore once (back it up; losing it forfeits app identity):

```
keytool -genkey -v -keystore ~/.keystores/wtranscriber-release.jks \
  -alias wtranscriber -keyalg RSA -keysize 4096 -validity 10000
```

## Build speed

Warm rebuild after one Rust source change (Windows, 16 cores):

| Recipe           | Time | Output                                  |
| ---------------- | ---- | --------------------------------------- |
| `just dev`       | live | hot-reload UI                           |
| `just build-bin` | 6s   | `target/release/wtranscriber.exe` (raw) |
| `just build-app` | 9s   | Tauri-patched exe, no installer         |
| `just build`     | 28s  | NSIS installer                          |
| `just build-all` | ~45s | NSIS + MSI                              |
| `just watch`     | live | rebuild on save                         |

Cold build: ~210 s. Floor is the single-threaded link of statically-bundled `sherpa-onnx`.

`[profile.release]`: `lto = false`, `codegen-units = 16`, `incremental = true`, `strip = "debuginfo"`. Heavy work is C++ (sherpa-onnx, webkit), so Rust LTO costs minutes per build for sub-1% runtime gain. Do not re-enable LTO. Do not cap `CARGO_BUILD_JOBS`.

WSL: install `mold` for ~6× faster linking:

```
sudo apt install mold clang
cat >> ~/.cargo/config.toml <<'EOF'
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]
EOF
```

## CI compliance roadmap (out of local scope)

For SLSA L3 / EU CRA / US EO 14028 add a GitHub Actions workflow calling `scripts/release-build.mjs` and `scripts/release-publish.sh` plus:

- `permissions: id-token: write` for Sigstore keyless OIDC
- `cosign attest --type cyclonedx` (CycloneDX SBOM via `cargo cyclonedx` + `cdxgen`)
- `slsa-framework/slsa-github-generator` for build provenance
- Windows Authenticode signing (Azure Trusted Signing or EV cert + signtool)
- `dpkg-sig --sign builder` for `.deb`
- macOS notarization (when added)
