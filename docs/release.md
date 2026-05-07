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
