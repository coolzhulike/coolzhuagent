---
name: coolzhu-goal-session-chain
description: Keep COOLZHU Goal execution attached to the correct workspace, chat room, session, artifact paths, and phase handoffs. Use for every Goal phase, scheduled Goal continuation, retry, or verification pass.
---

# COOLZHU Goal Session Chain

Apply these checks before executing a Goal phase and again before reporting completion.

## Resolve the execution chain

1. Identify the active Goal, phase, assigned session, and Goal chat room from the supplied context.
2. Keep replies, reasoning, tool evidence, and scheduled-task progress attached to that Goal room. Do not silently switch to the currently visible room or another target session.
3. Treat the active workspace and repository root as separate until verified. The runtime workspace may be `~/coolzhuagent` while the source repository is elsewhere.
4. Resolve every write target against the verified repository or workspace named by the task. Never infer that a relative artifact path is relative to the process working directory.

## Handle artifacts and handoffs

- Keep schema fields such as `output_artifacts` workspace-relative when the Goal API requires relative paths.
- Use verified absolute paths for actual filesystem writes when workspace and repository roots differ.
- After every write, confirm the exact file exists and contains meaningful final content.
- Preserve the phase dependency chain. Report produced artifacts, verification evidence, and blockers in a form the next phase can consume.
- On retry, repair the exact missing or invalid path from the previous verification message before doing unrelated work.
- Do not create a new room, retarget the conversation, or rename the scheduled-task room unless the phase explicitly requires it.

## Completion gate

Before reporting completion, state:

- Goal and phase being completed.
- Assigned session and room used.
- Verified repository/workspace root.
- Concrete artifact paths.
- Verification command or file evidence.

If any item is unknown, continue discovery or report a blocker instead of claiming completion.
