---
name: install-and-test
description: Install WTranscriber on Windows (GUI + CLI), Android, and WSL, then verify each successfully-installed target with a 30-second audio clip. Both steps run via wt-runner; the second reads tmp/install-report.json and skips any target whose install status is not `pass`.
steps:
  - agent: wt-runner
    task: |
      mode: install

      Install WTranscriber on all four targets: Windows GUI (NSIS), Windows CLI (wt.exe), Android (APK to attached device), and WSL Linux CLI (build wt headless binary inside WSL).

      Skip any target whose prerequisite is missing (no APK in releases/dev/, no device attached, no WSL distro). Skip ≠ failure.

      Output contract: tmp/install-report.json with `{ branch, results: { win_gui, win_cli, android, wsl_cli: { status, detail, binary|package } } }`. Missing artefact = the run did not happen.
    output: false

  - agent: wt-runner
    task: |
      mode: test

      Read tmp/install-report.json. Verify only the targets whose status is `pass`; skip the rest.

      Audio: tmp/test-30s.m4a (30.0 s).
      Reference keywords for CLI assertions (≥2 case-insensitive matches): art, music, creative, teachers, sports, draws, practice.
      Optional comparison: tmp/test-30s_sherpa-whisper-turbo_*.json from a known-good Windows CLI run.

      Output contract: tmp/test-report.json with per-target `{ status, transcript|screenshot, matched_keywords?, elapsed_s }` plus overall `PASS|FAIL`. Print summary table.

      Failures: do not edit code or commit. Stop with a FIX block. The orchestrator routes failures to wt-diagnose; edits and commits happen on the main thread.
    output: false
---

# install-and-test

Install + smoke-test cycle for WTranscriber across Windows GUI, Windows CLI, Android, and WSL Linux. Both steps are `wt-runner`; the handoff is artefact-based via `tmp/install-report.json`.

## Prerequisites

- `tmp/test-30s.m4a` exists (trim manually with ffmpeg).
- `releases/dev/` populated (run `just release` first if missing APK / installer).
- Android device with USB debugging on, plus `adb` on PATH.
- WSL distro with `cargo` and `bun` installed.

## Run

    /run-chain install-and-test

## Outputs

- `tmp/install-report.json` — install phase artefact (read by step 2).
- `tmp/test-report.json` — test phase artefact + overall verdict.
- `tmp/win-gui.png` — Windows GUI screenshot.
- `tmp/android.png` — Android screenshot.
