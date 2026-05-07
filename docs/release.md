# Release process

## TL;DR

```bash
just release           # dev build → 'dev' rolling prerelease tag on GitHub
just release-stable    # bump patch, tag vX.Y.Z, build, publish stable release
```

`release-stable level=minor` / `level=major` for non-patch bumps.
`release-stable level=1.2.3` to set an exact version.

## Channels

| Channel | Tag             | Type           | Mutability                    | Filenames                                |
| ------- | --------------- | -------------- | ----------------------------- | ---------------------------------------- |
| dev     | `dev`           | Pre-release    | Force-updated every build     | `wtranscriber-setup-<branch>.exe`, etc.  |
| stable  | `vX.Y.Z`        | Release        | **Immutable** once published  | `wtranscriber-setup-<ver>.exe`, etc.     |

Stable filenames embed the SemVer version so each release is independently
addressable. Dev filenames embed the branch so download URLs are stable
across rolling builds.

## What gets built and published

Per channel: host platform artifacts (Windows NSIS installer **or** Linux
`.deb` / `.AppImage` / `.rpm`) + Android APK + `SHA256SUMS` + a JSON
`release-manifest.json` capturing `version / sha / branch / builtAt`.

If `TAURI_SIGNING_PRIVATE_KEY` is exported, `.sig` files are produced for
each binary so you can later enable the Tauri updater plugin without a
re-release.

## Versioning policy — SemVer 2.0

- All public versions are strict `MAJOR.MINOR.PATCH`.
- Pre-release builds carry the `-dev.<sha>` suffix in metadata only; on-disk
  filenames use `<branch>` for stable URLs.
- Three files are kept in lockstep by `scripts/release-bump.mjs`:
  - `package.json` (`version`)
  - `src-tauri/Cargo.toml` (`[package].version`)
  - `src-tauri/tauri.conf.json` (`version`)
- `Cargo.lock` is refreshed via `cargo update -w --offline` after the bump.
- Stable tags are **never** force-pushed. Only `dev` is rewritten.

## Quality gate (mandatory before stable)

`just release-stable` runs `just check` first:

1. `cargo fmt --check` + `prettier --check`
2. `cargo clippy --all-targets --offline -D warnings`
3. `bun run typecheck` (`vue-tsc`)
4. `cargo test --offline`

A dirty working tree blocks the release.

## Android signing (required for stable APKs)

Release APKs **must** be signed with v2/v3 (apksigner) for Android to
install them. The release script will refuse to publish unsigned APKs on
the stable channel and warn (but proceed) on dev.

### One-time setup

1. Generate a keystore (back this up — losing it means losing your app's
   identity on the Play Store):

   ```bash
   keytool -genkey -v \
     -keystore ~/.keystores/wtranscriber-release.jks \
     -alias wtranscriber -keyalg RSA -keysize 4096 -validity 10000
   ```

2. Create `src-tauri/gen/android/keystore.properties` (gitignored — see
   `keystore.properties.example`):

   ```properties
   storeFile=/abs/path/to/wtranscriber-release.jks
   storePassword=...
   keyAlias=wtranscriber
   keyPassword=...
   ```

3. The first release build runs `scripts/patch-android-signing.mjs` to
   inject a `signingConfigs.release` block into the (gitignored)
   `gen/android/app/build.gradle.kts`. The patch is idempotent and survives
   `tauri android init` regenerations.

If `keystore.properties` is absent, the script invokes `zipalign` +
`apksigner` directly using the same fields, so the signing path works
either way.

## Release notes

Stable releases use `gh release create --generate-notes`, which produces
notes from PR titles + commits since the previous tag. Use Conventional
Commits (`feat:`, `fix:`, `chore:`, `docs:`, `BREAKING CHANGE:`) for clean
output.

## Integrity

Every published release ships a `SHA256SUMS` (dev) or `SHA256SUMS-<ver>`
(stable) file containing hashes of every artifact. Verify with:

```bash
sha256sum -c SHA256SUMS-1.2.3
```

## What's still required for full supply-chain compliance

The local flow above covers SemVer, immutability, integrity hashes, and
signed APKs. To reach SLSA Build Level 3 / EU CRA / US EO 14028 posture
you should additionally wire in (CI-only, out of scope for `just`):

- **GitHub Actions release workflow** with `permissions: id-token: write`
  to enable keyless signing.
- **Sigstore cosign** keyless attestation of every artifact + SBOM
  (`cosign attest --type cyclonedx`).
- **CycloneDX SBOM** generation: `cargo cyclonedx --format json` for Rust,
  `bun pm ls --json` (or `cdxgen`) for the JS side.
- **SLSA provenance** via `slsa-framework/slsa-github-generator`.
- **Windows Authenticode** signing of `.exe` installers (EV cert + signtool;
  GitHub-hosted runners support azure-trusted-signing).
- **Linux deb signing** (`dpkg-sig --sign builder *.deb`) + repo metadata.
- **macOS notarization** if/when a Mac target is added.
- **`SECURITY.md`** with disclosure contact + supported version table
  (CRA Article 13 obligation).

These are wired in the typical layout:

```
.github/workflows/release.yml   # cosign + SLSA + Authenticode + SBOM
SECURITY.md                     # disclosure policy
```

The justfile flow is intentionally local-first; the CI workflow can call
the same `release-build.mjs` and `release-publish.sh` so logic stays in
one place.

## Manual recipes

```bash
just release-bump patch         # bump + commit + tag, no build/publish
just release-build --dev        # build artifacts only (dev naming)
just release-build              # build artifacts only (stable naming)
just release-publish dev        # upload existing releases/dev/* to 'dev' tag
just release-publish stable     # push HEAD+tag, create release, upload
```

## Build speed — lazy rebuilds

Measured warm-rebuild times after touching one Rust source file:

| Recipe          | Time | Output                        | Use case                          |
| --------------- | ---- | ----------------------------- | --------------------------------- |
| `just dev`      | live | hot-reload UI                 | Iterate on Vue / Tauri commands   |
| `just build-bin`| **6s** | `target/release/wtranscriber.exe` (raw cargo, no Tauri post-process) | Rust changes, run binary directly |
| `just build-app`| **9s** | Tauri-patched exe (icon, metadata, no installer) | Test packaged exe behavior        |
| `just build`    | **28s**| NSIS installer                | Pre-release verification          |
| `just build-all`| ~45s | NSIS + MSI                    | Legacy / enterprise GPO deploy    |
| `just watch`    | live | auto rebuild on save          | Continuous iteration              |

### What changed (vs default Tauri template)

1. **`[profile.release]` tuned for build speed, not micro-perf**: `codegen-units = 16`,
   `lto = false`, `incremental = true`, `strip = "debuginfo"`. The heavy lifting in this
   app is C++ (sherpa-onnx, webkit), so Rust-level LTO + single-codegen-unit cost minutes
   to save microseconds at runtime.
2. **Default `bundle.targets` = `["nsis", "deb", "app"]`** instead of `"all"`. MSI takes
   ~15s of WiX work for a target almost nobody uses (enterprise GPO). `just build-all`
   restores it.
3. **`src-tauri/.cargo/config.toml`**: sparse registry + `git-fetch-with-cli` + global
   `incremental = true`.
4. **Lazy escape hatches**: `build-bin` skips Tauri entirely; `build-app` skips installer
   bundling. Use these for inner-loop iteration.

### What does NOT speed up

- The cold first build is ~3.5 min, dominated by **single-threaded link of statically-bundled
  sherpa-onnx**. AGENTS.md flags this as expected. Switching to a dynamically-linked sherpa
  build would help but requires shipping `sherpa-onnx-c-api.dll` separately.
- Don't add `CARGO_BUILD_JOBS=1` — AGENTS.md mandates not capping cargo parallelism.
- Don't enable `[profile.release] lto = true` again unless you're profiling and proving it
  matters; it adds 2-3 minutes per build for sub-1% runtime gain in this app.

### Squeezing more on Linux / WSL

Install `mold` linker — drops link time from ~30s to ~5s for the wtranscriber binary:

```bash
sudo apt install mold clang
mkdir -p ~/.cargo && cat >> ~/.cargo/config.toml <<'EOF'
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]
EOF
```

Windows: rust-lld is bundled with the toolchain but linking statically with sherpa-onnx
is risky (long stalls per AGENTS.md). Default `link.exe` is what we ship with.
