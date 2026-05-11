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

## Windows VM preflight

The Linux host builds the Windows NSIS installer by SSH-driving a Tiny11
VM hosted in `~/os/windows-vm` (dockur/windows). `xtask release`
resolves the VM via the SSH alias `windows-vm` (`localhost:2222`).

```bash
cd ~/os/windows-vm && make start    # resume the VM (idempotent)
ssh windows-vm true                 # smoke-test; xtask retries for ~60 s
```

If SSH probes get `kex_exchange_identification: Connection reset by
peer`, the guest sshd is wedged behind a sign-in screen — run
`make -C ~/os/windows-vm ssh-restart` (drives `Restart-Service sshd`
over VNC). Without a healthy alias, `xtask release` warns
`windows-vm SSH unreachable — skipping Windows build` and continues
without the `.exe`.

> If SSH still won’t connect after `ssh-restart`, the Windows guest is
> frozen. Reboot it via the web viewer (http://127.0.0.1:8006/) and
> wait 2–3 min for sshd to come back.

### Failsafe

`xtask release` is self-healing for transient failures: if the initial
SSH probe fails, or if the Windows build inside the VM exits non-zero,
the task runs `docker restart windows`, polls SSH for up to 5 min, and
retries the build **once** before giving up. The build script
(`scripts/wt-windows-build.bat`) attempts a softer self-heal first —
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

(See `~/os/windows-vm/shared/fix-rustup.ps1`. It deletes `~/.rustup`
and `~/.cargo/bin`, reinstalls via `rustup-init.exe`, and re-adds the
msvc target. Same access pattern as `restart-sshd.ps1`: web viewer
→ admin shell → run.) Then re-run `just release`.

Full setup and recovery flow: see `~/os/windows-vm/AGENTS.md`.

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

| Stage             | Gate                                                                                                                                        |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `release-bump`    | Working tree clean; tag does not exist                                                                                                      |
| `release-stable`  | `just check` (11 parallel jobs: fmt-check, clippy, clippy-xtask, typecheck, vue-lint, knip, rust-test, xtask-test, js-test, machete, audit) |
| `release-publish` | Stable: clean tree, local tag exists                                                                                                        |
| `release-build`   | Stable: refuses unsigned APK; dev: warns and continues                                                                                      |

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
