# Agent Tool Calling Guide

This file is the local reference sheet for model sessions before they call tools.
Before any tool call, check the matching demo and pass parameters exactly as the
tool schema requires.

## Hard Rules

- Use the tool's top-level JSON fields directly. Do not wrap arguments in
  `raw` unless the tool schema explicitly requires a `raw` field.
- If a tool returns `missing field`, immediately retry with the required fields
  at the top level.
- For file creation or edits, prefer `write_file` or `edit_file` over shell
  redirection.
- For Windows shell work, prefer `PowerShell`. Use `bash` only for commands that
  are valid in the current shell runtime.
- Shell commands are OS-specific:
  - Windows: use `PowerShell` (`New-Item`, `Set-Content`, `Get-ChildItem`,
    `Test-Path`). Do not send POSIX-only syntax such as `mkdir -p`, heredocs,
    `cat > file <<EOF`, or bare `ls` to the Windows `bash` compatibility path.
  - Linux/macOS: use `bash`/POSIX shell syntax unless the task explicitly needs
    PowerShell.
- A successful directory creation is not a completed file task. If the expected
  artifact is a file, call `write_file`, `edit_file`, `PowerShell`, or `bash`
  until the exact file exists and has meaningful content.
- After writing a required artifact, verify with `read_file` or an explicit file
  existence/content check.

## Working Approach

- Plan first: for multi-step tasks, use `grep_search`/`glob_search` to discover the
  relevant context and outline the steps before acting. Do not assume file names or
  APIs — verify them.
- Call tools in parallel: batch independent `read_file`/`glob_search`/`grep_search`
  calls in a single turn instead of one at a time.
- Minimal change: do only what was asked. Do not over-engineer, add unrequested
  abstractions, or refactor unrelated code.
- Verify with signals: after changing code, run the relevant build/test and check the
  exit code — do not rely on "looks correct". For a Goal phase with Command
  verification, a non-zero exit auto-routes the phase back to implementer; fix until
  it passes.
- Be persistent: do not hand back before the task is fully done and verified. When
  blocked, diagnose the root cause — do not retry the same failing command in a loop.
- Decide vs ask: in autonomous contexts (Goal phases) proceed with sensible defaults
  instead of waiting for input; confirm first only for destructive or outward-facing
  actions (delete, overwrite, push).

## Common Tool Demos

| Tool | Use for | Correct parameters |
| --- | --- | --- |
| `write_file` | Create or replace a text file | `{"path":"goal-artifacts/demo.html","content":"<!doctype html><html><body>ok</body></html>"}` |
| `edit_file` | Replace known text in a file | `{"path":"src/app.js","old_string":"old text","new_string":"new text","replace_all":false}` |
| `read_file` | Read a text file | `{"path":"goal-artifacts/demo.html","offset":0,"limit":200}` |
| `glob_search` | Find files by glob | `{"pattern":"goal-artifacts/**/*.html","path":"."}` |
| `grep_search` | Search file contents | `{"pattern":"write_file","path":"modules","include":"*.rs"}` |
| `bash` | Run a POSIX shell command on Linux/macOS, or a Windows cmd-compatible command only when necessary | `{"command":"dir goal-artifacts","timeout":30000,"description":"List artifacts"}` |
| `PowerShell` | Run a PowerShell command on Windows | `{"command":"Get-ChildItem -LiteralPath goal-artifacts","timeout":30000,"description":"List artifacts"}` |
| `WebSearch` | Search the web | `{"query":"2026 mainstream AI models comparison"}` |
| `WebFetch` | Fetch a URL | `{"url":"https://example.com","prompt":"Summarize the page."}` |
| `chat_handoff` | Send a task to another configured chat agent | `{"to":"test3","intent":"Verify the output artifact","attach":"recent"}` |
| `tools_semantic_dispatch` | Legacy semantic dispatch or dry-run planning | `{"intent":"create file demo.txt with hello","execute":true,"target":"demo.txt","text":"hello"}` |

## Bad Patterns To Avoid

Do not call:

```json
{"raw":"{\"path\":\"demo.html\",\"content\":\"ok\"}"}
```

Call:

```json
{"path":"demo.html","content":"ok"}
```

Do not call:

```json
{"raw":"{\"command\":\"mkdir -p goal-artifacts\"}"}
```

Call:

```json
{"command":"New-Item -ItemType Directory -Force -Path goal-artifacts","timeout":30000}
```

## Goal Artifact Checklist

For a Goal phase with `FilesExist` verification:

1. Identify every required artifact path.
2. Create parent directories if needed.
3. Write the exact artifact file with meaningful content.
4. Read or inspect the exact artifact path.
5. Report the concrete path and verification evidence.
