---
name: install-and-test
description: Install WTranscriber on Windows (GUI + CLI), Android, and WSL, then verify each platform with a 30-second audio clip.
steps:
  - agent: wt-installer
    task: |
      Install WTranscriber on all four targets: Windows GUI (NSIS), Windows CLI (wt.exe), Android (APK to attached device), and WSL Linux CLI (build wt headless binary inside WSL).

      The 30-second test clip is already at tmp/test-30s.m4a. Audio source was C:\Users\asolo\Desktop\fulham-boys-school-admission-interview_260505-161101.m4a.

      Skip a target gracefully if its prerequisite is missing (no APK file, no device, no WSL distro). Write tmp/install-report.json and print a summary table.
    output: false

  - agent: wt-tester
    task: |
      Verify functionality on each target that successfully installed (read tmp/install-report.json).

      Audio: tmp/test-30s.m4a (30.0 s).
      Reference keywords for CLI assertions (need ≥2 case-insensitive matches): art, music, creative, teachers, sports, draws, practice.
      Reference text from a known-good Windows CLI run is in tmp/test-30s_sherpa-whisper-turbo_*.json - you may compare against it.

      Write tmp/test-report.json and print a summary plus overall PASS/FAIL verdict.
    output: false
---

# install-and-test

Install + test cycle for WTranscriber across Windows GUI, Windows CLI, Android, and WSL Linux.

## Prerequisites

- `tmp/test-30s.m4a` exists (run `just test-prep` or trim manually)
- `releases/dev/` populated (run `just release` first if missing APK / .deb)
- Android device with USB debugging on, plus `adb` on PATH
- WSL distro with `cargo` and `bun` installed

## Run

    /run-chain install-and-test

## Outputs

- `tmp/install-report.json`
- `tmp/test-report.json`
- `tmp/win-gui.png` (Windows GUI screenshot)
- `tmp/android.png` (Android screenshot)
