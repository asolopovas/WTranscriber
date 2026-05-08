# Agent dev loop

Module of [`AGENTS.md`](../AGENTS.md). Defines which subagents run during development, what each owns, and how they communicate. For the HMR/CDP mechanics see [`dev-loop.md`](dev-loop.md).

## Roles

Three roles, sharp boundaries. Overlap is a decomposition bug.

| Role          | Agent                  | Lifecycle              | Owns                              |
| ------------- | ---------------------- | ---------------------- | --------------------------------- |
| **Monitor**   | `error-monitor.mjs`    | long-running, async    | `tmp/error-monitor.log` (write)   |
| **Fixer**     | `doctor` (diag mode)   | short-lived, on demand | source under `src/`, `src-tauri/` |
| **Committer** | `doctor` (commit mode) | short-lived, gated     | git index, branch, remote         |

Main thread is the **coordinator** — decomposes work, spawns agents, never greps logs or runs `just check` itself.

## Why three, not one

- **Monitor must not block.** Long-running, can't return — runs `async: true` with `control: { enabled: false }`.
- **Fixer must not commit.** Mixing fix + commit hides regressions: a fix that breaks something else still gets pushed because the same context rationalized both. Always re-gate via committer.
- **Committer must run the gate.** `.githooks/pre-commit` is mandatory; the committer's job is to hit it and never bypass with `--no-verify`.

## Startup sequence

Run at the start of every dev session, in this order:

```
1. just android-dev               # user's terminal, leave running (HMR, --no-watch)
2. just android-debug-attach      # once, after WebView is up (forwards CDP :9222)
3. spawn monitor as async subagent (snippet below)
```

Desktop dev (`just dev`) skips step 2; the monitor's CDP side is no-op without `:9222` and just streams Rust stderr.

### Monitor spawn

```js
subagent({
  agent: "delegate",
  task: "node scripts/error-monitor.mjs\n\nStream forever. Surface any error/warn line. Ignore inactivity warnings.",
  async: true,
  cwd: "C:/Users/asolo/src/WTranscriber",
  control: { enabled: false },
});
```

Reattaches automatically across `just android-install` (CDP retries ~2 min).

## Communication: filesystem, not events

Claude Code / Codex async subagents **cannot wake the parent on completion** (anthropics/claude-code#20921, openai/codex#15723). The monitor therefore signals via append-only file:

- Monitor writes one filtered, deduped line per error to `tmp/error-monitor.log`.
- Main thread checks line count between turns (post-edit, pre-commit, on user prompt).
- New lines → spawn fixer with the **error excerpt**, not the whole log.

Never pipe the monitor directly into the fixer. The excerpt is the contract.

## Handoff chain

```
monitor (async, forever)
  └─ appends tmp/error-monitor.log
       └─ main thread polls between turns
            └─ on new error → fixer (doctor, diag mode)
                 task: "Error: <excerpt>. Diagnose and fix. Do not commit."
                 returns: VERDICT / EVIDENCE / FIX (+ edits applied)
                 └─ on green → committer (doctor, commit mode)
                      task: "Commit: <fixer summary>."
                      runs pre-commit gate, conventional message, push
                      returns: commit hash
```

## Delegation rules

- **No log grepping in main thread.** Hand `doctor` a focused question; it reads `tmp/error-monitor.log`, runs CDP probes, returns a verdict.
- **No `just check` in main thread.** `doctor` absorbs the multi-minute output and surfaces only decisions.
- **All commits go through the committer.** Even trivial ones. Pass change summary; it stages deliberately, gates, writes the message, pushes, returns the hash.
- **One file, one owner per parallel run.** If two fixers could touch the same file, serialize them or use git worktrees (`worktree: true` in parallel subagent calls).
- **Every invocation leaves an artifact** (`tmp/*.log`, commit hash, verdict). Replay is otherwise impossible.

## Additional project agents

| Agent          | When to run                                                         |
| -------------- | ------------------------------------------------------------------- |
| `wt-installer` | After release build, before announcing — Win GUI/CLI, Android, WSL. |
| `wt-tester`    | After install verification — 30-second-clip smoke across platforms. |

Chained: `.pi/chains/install-and-test.chain.md`.

## Checklist

Before any non-trivial edit session:

- [ ] HMR running (`just android-dev` or `just dev`).
- [ ] CDP attached (Android only).
- [ ] Monitor spawned async; `tmp/error-monitor.log` exists and growing.
- [ ] Pre-commit hook installed (`git config core.hooksPath .githooks`).

If any box is unchecked, fix it before editing — debugging without the monitor is the slow path.
