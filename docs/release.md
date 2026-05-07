# Release

## Recipes

| Recipe                        | Action                                                              |
| ----------------------------- | ------------------------------------------------------------------- |
| `just release`                | Dev build → rolling `dev` prerelease (force-updated)                |
| `just release-stable [level]` | `check` → bump (`patch`\|`minor`\|`major`\|`X.Y.Z`) → tag → publish |
| `just release-bump [level]`   | Bump + commit + tag only                                            |
| `just release-build [--dev]`  | Build artifacts only (`releases/[dev/]`)                            |
| `just release-publish <ch>`   | Upload existing `releases/[dev/]*` to `dev` or `vX.Y.Z`             |

Flags: `--no-host`, `--no-android`, `--no-wsl`, `--skip-rebuild`, `--sequential`.

## Channels

| Channel | Tag      | Filenames                                                           | Mutability        |
| ------- | -------- | ------------------------------------------------------------------- | ----------------- |
| dev     | `dev`    | `wtranscriber-setup-<branch>.exe`, `wtranscriber-<branch>.apk`, ... | rolling           |
| stable  | `vX.Y.Z` | `wtranscriber-setup-<ver>.exe`, `wtranscriber-<ver>.apk`, ...       | **immutable**     |

## Per-release artifacts

- Windows host → `wtranscriber-setup-*.exe` (NSIS)
- Linux host → `wtranscriber_*_amd64.deb`
- Windows + WSL → `.deb` built in WSL at `$HOME/.cache/wtranscriber-wsl-target/`
- Android → `wtranscriber-*.apk` (signed if `keystore.properties` present)
- `SHA256SUMS[-<ver>]`, `release-manifest-<ver>.json`
- `.sig` per binary if `TAURI_SIGNING_PRIVATE_KEY` exported

## Gates

| Stage              | Gate                                                                |
| ------------------ | ------------------------------------------------------------------- |
| `release-bump`     | Clean tree; tag `vX.Y.Z` must not exist                             |
| `release-stable`   | `just check` (fmt + clippy + typecheck + test) before bump          |
| `release-publish`  | Stable: clean tree + tag exists locally                             |
| `release-build`    | Stable: refuses unsigned APK; dev: warns                            |

Bump is committed with `--no-verify` (gate already ran via `just check`).

## Version sync (on bump)

`package.json` · `src-tauri/Cargo.toml` · `src-tauri/tauri.conf.json` · `src-tauri/Cargo.lock` (via `cargo update -w --offline`).

## Android signing

Required for stable, recommended for dev.

```
src-tauri/gen/android/keystore.properties      (gitignored; see .example)
  storeFile=
  storePassword=
  keyAlias=
  keyPassword=
```

`scripts/patch-android-signing.mjs` runs before each Android build. Idempotent; injects `signingConfigs.release` into the generated `app/build.gradle.kts`. Outside-keystore fallback: `apksigner` invoked directly with the same fields.

Generate keystore once (back it up — losing it forfeits app identity):

```
keytool -genkey -v -keystore ~/.keystores/wtranscriber-release.jks \
  -alias wtranscriber -keyalg RSA -keysize 4096 -validity 10000
```

## Build speed

Warm rebuild after one Rust source change (Windows, 16 cores):

| Recipe           | Time   | Output                                  |
| ---------------- | ------ | --------------------------------------- |
| `just dev`       | live   | hot-reload UI                           |
| `just build-bin` | **6s** | `target/release/wtranscriber.exe` (raw) |
| `just build-app` | 9s     | tauri-patched exe (no installer)        |
| `just build`     | 28s    | NSIS installer                          |
| `just build-all` | ~45s   | NSIS + MSI                              |
| `just watch`     | live   | auto rebuild on save                    |

Cold build: ~210s (link of statically-bundled sherpa-onnx is the floor; single-threaded).

Profile in `src-tauri/Cargo.toml` is tuned for build speed (`lto = false`, `codegen-units = 16`, `incremental = true`, `strip = "debuginfo"`). Heavy lifting is C++ (sherpa-onnx, webkit), so Rust-level LTO is sub-1% runtime gain at minutes-per-build cost. **Do not** re-enable LTO or cap `CARGO_BUILD_JOBS` (per AGENTS.md).

WSL: install `mold` for ~6× link speedup:

```
sudo apt install mold clang
cat >> ~/.cargo/config.toml <<'EOF'
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]
EOF
```

## CI compliance roadmap (out of local scope)

For SLSA L3 / EU CRA / US EO 14028 add a GitHub Actions workflow that calls
`scripts/release-build.mjs` + `scripts/release-publish.sh` plus:

- `permissions: id-token: write` for Sigstore keyless OIDC
- `cosign attest --type cyclonedx` (CycloneDX SBOM via `cargo cyclonedx` + `cdxgen`)
- `slsa-framework/slsa-github-generator` for provenance
- Windows Authenticode (Azure Trusted Signing or EV cert + signtool)
- `dpkg-sig --sign builder` for `.deb`
- macOS notarization (when target added)
