# Tauri Android Debugging

Module of [`AGENTS.md`](../AGENTS.md). Covers inspection of an already-running Android build (WebView DevTools, CDP, logcat). For HMR-based UI dev see [`android.md`](android.md) section "Live UI dev".

## Remote WebView DevTools

Tauri 2 enables Chromium WebView remote debugging in debug builds automatically (`WebView.setWebContentsDebuggingEnabled(true)`). Production builds need a custom hook.

### One-shot

```bash
just android-debug-attach
```

Recipe steps:

1. Finds the WebView abstract socket in `/proc/net/unix` (`webview_devtools_remote_<pid>`).
2. Runs `adb forward tcp:9222 localabstract:webview_devtools_remote_<pid>`.
3. Prints the page list.

Then open `chrome://inspect/#devices` (or `http://localhost:9222`) on the host.

### Manual

```bash
adb shell cat /proc/net/unix | grep webview_devtools_remote
# webview_devtools_remote_19670 indicates PID 19670
adb forward tcp:9222 localabstract:webview_devtools_remote_19670
curl -s http://localhost:9222/json/list
```

## Headless CDP

`scripts/cdp.mjs` connects via `playwright.chromium.connectOverCDP("http://localhost:9222")` and evaluates an expression on the active page.

```bash
node scripts/cdp.mjs "({ua:navigator.userAgent, audio:typeof AudioContext})"
node scripts/cdp.mjs "Array.from(document.querySelectorAll('button[title]')).map(b=>b.title)"
node scripts/cdp.mjs "(()=>{const b=document.querySelector('button[title=\"Play selection\"]'); b.click(); return b.disabled})()"
```

Use cases: probe reactive state, dispatch clicks, inspect computed CSS, capture console errors.

## Logcat

```bash
adb logcat -c                                              # clear
adb logcat -v time '*:S' chromium:V Console:V              # JS console + chromium
adb logcat -v time '*:S' RustStdoutStderr:V wtranscriber:V # Rust println + log crate
```

`tauri-plugin-log` with `Target::Stdout` pipes to logcat tag `RustStdoutStderr`. `Target::Webview` mirrors to JS console.

## Screenshots

```bash
export MSYS_NO_PATHCONV=1   # Git Bash on Windows
adb exec-out screencap -p > tmp/wt.png
```

`*.png` in repo root is gitignored; keep captures under `tmp/`.

## CDP probes

```bash
# WebView codec support
node scripts/cdp.mjs "Object.fromEntries(['audio/aac','audio/mp4;codecs=mp4a.40.2','audio/wav','audio/ogg'].map(t=>[t,document.createElement('audio').canPlayType(t)]))"

# Decode a file via Web Audio
node scripts/cdp.mjs "(async()=>{const r=await fetch('http://asset.localhost/sdcard/Documents/WTranscriber/x.m4a'); const b=await r.arrayBuffer(); const ctx=new AudioContext(); const buf=await ctx.decodeAudioData(b); return {dur:buf.duration, ch:buf.numberOfChannels, sr:buf.sampleRate};})()"

# Trigger a Tauri command
node scripts/cdp.mjs "window.__TAURI_INTERNALS__.invoke('app_version').then(v=>v)"
```

## References

- Tauri debug: https://tauri.app/develop/debug
- Chrome remote WebView: https://developer.chrome.com/docs/devtools/remote-debugging/webviews
- Logging plugin: https://v2.tauri.app/plugin/logging
- `tauri-plugin-log`: https://crates.io/crates/tauri-plugin-log
