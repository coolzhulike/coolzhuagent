---
name: coolzhu-goal-tool-execution
description: Enforce correct tool schemas, Windows command discipline, temporary script/log placement, and verification for COOLZHU Goal phases. Use before every Goal phase tool call, command execution, file write, or build/test step.
---

# COOLZHU Goal Tool Execution

Tool calls must be exact, observable, and reversible enough for supervision.

## Tool schema discipline

- Pass tool arguments exactly as top-level fields required by the tool schema.
- Do not wrap tool parameters in `raw` unless the schema explicitly asks for free-form raw input.
- On a missing-field or schema error, read the schema again and retry with corrected top-level fields.
- Prefer direct file tools for source edits. Use shell commands for build, test, inspection, or when no direct file tool is available.

## Windows and repository discipline

- Assume the host shell is PowerShell on Windows unless the prompt explicitly states otherwise.
- Use Windows paths correctly. Do not mix POSIX path assumptions into PowerShell commands.
- Verify the repository root and active workspace before writing.
- When a phase output path is relative for the Goal schema, still resolve the actual write path against the verified source repository or intended workspace.

## Temporary script and log rules

- Put temporary scripts under `tmp/`.
- Redirect command, build, and script output to `tmp/logs/`.
- Every script or long command must have an explicit timeout.
- If a command fails, inspect the log file first and fix the root cause before retrying.
- Avoid noisy marker output; logs should be useful evidence, not decoration.

## Build and verification rules

- For Rust/web-console changes, remember `include_str!` may cache frontend files; touch or rebuild `main.rs` when needed so frontend changes are picked up.
- Run the relevant `cargo test`, not only `cargo build`, when tests or `#[cfg(test)]` code are involved.
- After a model-driven change, inspect `git diff --name-only` and changed hunks for scope violations.
- Verify that artifacts are files with meaningful content, not just directories.
- Record exact commands, timeouts, logs, exit codes, and produced artifacts in the phase report.

## Failure handling

If verification fails:

1. Read the failing log.
2. Identify the concrete root cause.
3. Make the smallest scoped fix.
4. Re-run the same verification with a timeout.
5. Report both the failed and passing evidence.
