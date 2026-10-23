# HA-00 Baseline Restore

- Created: 2026-10-18T05:25:56.567886+00:00
- HEAD: `8a13551b037f5f673188802f2c9128984245fab8`
- Source patch: `source.patch` (659732 bytes)
- Untracked source files: 29
- Included changed text files: 72
- Excluded binary changed files: 0
- Excluded untracked binary files: 0

This is the **implementation baseline** for HA-00. It is not the planning fingerprint
in `../baseline.json` and does not include runtime state, databases, secrets, logs,
build outputs, or binary assets not required by the Harness code/tests.

## Restore

```powershell
$repo = "D:\coolzhuagent"
$tmp = "D:\coolzhuagent\tmp\ha00-restore"
New-Item -ItemType Directory -Force $tmp | Out-Null
git -C $repo archive HEAD | tar -x -C $tmp
git -C $tmp apply --binary "$repo\docs\plans\harness-alignment\baseline\HA-00-20261018\source.patch"
Copy-Item "$repo\docs\plans\harness-alignment\baseline\HA-00-20261018\untracked\*" $tmp -Recurse -Force
```

Then verify the current working tree against `manifest.json` for the listed files.
The snapshot intentionally excludes binary assets not needed for code/test builds.
