# HA-00 Baseline Restore

- Created: 2026-10-18T05:45:15.120221+00:00
- HEAD: `8a13551b037f5f673188802f2c9128984245fab8`
- Source patch: `source.patch` (660858 bytes)
- Untracked source files: 29
- Included changed text files: 73
- Excluded binary changed files: 0
- Excluded untracked binary files: 0

This is the **implementation baseline** for HA-00. It is not the planning fingerprint
in `../baseline.json` and does not include runtime state, databases, secrets, logs,
build outputs, or binary assets not required by the Harness code/tests.

## Restore

```powershell
$repo = "D:\coolzhuagent"
$baseline = Join-Path $repo "docs\plans\harness-alignment\baseline\HA-00-20261018"
$manifest = Get-Content -LiteralPath (Join-Path $baseline "manifest.json") -Raw | ConvertFrom-Json
$restoreRoot = Join-Path $repo "tmp\ha00-restore"
$archivePath = Join-Path $repo "tmp\ha00-restore.tar"
if ((Test-Path -LiteralPath $restoreRoot) -or (Test-Path -LiteralPath $archivePath)) {
    throw "恢复路径已存在，请选择新的隔离路径，不能覆盖现有工作。"
}
New-Item -ItemType Directory -Path $restoreRoot | Out-Null
# 固定 manifest 中的提交，避免 HEAD 漂移；输出文件避免 PowerShell 二进制管道转码。
git -C $repo archive --format=tar --output=$archivePath $manifest.git_head
if ($LASTEXITCODE -ne 0) { throw "git archive 失败" }
tar -xf $archivePath -C $restoreRoot
if ($LASTEXITCODE -ne 0) { throw "tar 解包失败" }
# 独立仓库避免 git apply 向上找到日常工作区。
git -C $restoreRoot init -q
if ($LASTEXITCODE -ne 0) { throw "git init 失败" }
git -C $restoreRoot apply --binary (Join-Path $baseline "source.patch")
if ($LASTEXITCODE -ne 0) { throw "git apply 失败" }
Copy-Item (Join-Path $baseline "untracked\*") $restoreRoot -Recurse
```

Then verify the **restored directory** against `manifest.json`: changed tracked
files use `git_blob_sha1` (Git's configured text normalization), untracked files
use SHA-256, and entries marked deleted must be absent. The current candidate
workspace is deliberately different and must not be used as the baseline hash.
The snapshot intentionally excludes binary assets not needed for code/test builds.

2026-10-18 校正：基线 manifest/source.patch/untracked 内容保持原样。恢复说明改用固定提交和独立目录。
本轮实际演练恢复了清单中的 73 项变更和 29 项未跟踪源码，共 102 项，核验 0 错误；
该演练是清单恢复验证，不代表已在恢复目录完成全量构建或真实模型对照。
