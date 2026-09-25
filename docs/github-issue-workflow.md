# GitHub issue / PR 工作流（GL-15）

本仓库的需求以 GitHub issue 管理、改动以 PR 交付。本文记录**实际可用**的操作路径与验证节奏。

## 1. 认证：不装 gh 也能做

`gh` CLI 在本机**没有安装**，且国内网络下安装不稳。实际走 **REST API + Git Credential Manager**
已缓存的凭证即可，无需任何人手输 token：

```bash
# 从凭证管理器取 token（不回显、不落盘）
TOKEN=$(printf "protocol=https\nhost=github.com\n\n" | git credential fill | sed -n 's/^password=//p')

curl -sS -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Accept: application/vnd.github+json" \
  --data-binary "@payload.json" \
  https://api.github.com/repos/<owner>/<repo>/issues
```

要点：

- **payload 走文件**（`--data-binary "@file.json"`），不要内联长 JSON——中文正文很容易超命令行长度上限，
  表现为 `Argument list too long` 或 GitHub 报 `For 'links/0/schema', nil is not an object`。
- Windows 原生 `curl` **不认 Git Bash 的 `/tmp` 映射**。payload 文件必须用 `C:/...` 真实路径，
  否则会静默发出空 body。
- 需要 `repo` scope。用 `curl -sS -H "Authorization: Bearer $TOKEN" https://api.github.com/user`
  看响应头 `x-oauth-scopes` 确认。

装了 `gh` 之后等价命令是 `gh issue create` / `gh pr create`，本文的 REST 路径可继续作为无 gh 环境的后备。

## 2. Issue 模板

`.github/ISSUE_TEMPLATE/` 下：

- `task.md`：背景 / 方案要点 / **验收标准(AC)** / 依赖 / 影响文件
- `bug.md`：现象 / 复现 / 期望 / 环境 / 证据

约定：正文中文；引用设计文档编号；AC 写成**可验证的结果**而不是"实现了 XX"，并说明如何验证。

## 3. 每轮交付的验证节奏

Goal 循环状态图化（GL-01~15）这一系列 PR 用的流程，建议沿用：

```
实现 → codex 审查 → 按反馈修 → 本地启动验证 → PR
```

**codex 审查**：把 diff + 背景 + 关键核查点喂给 codex。对改变行为的改动要求它**亲自拉起 app 做场景实测**，
而不是只读 diff——真实并发/边界请求能复现出纯静态审查发现不了的问题。

**本地启动验证**：用独立 `USERPROFILE` 起 app，生产库（`~/coolzhuagent`）绝不参与：

```bash
USERPROFILE=/path/to/isolated ./target/debug/coolzhu-web-console.exe
```

`user_home_dir()` 读 `USERPROFILE`，所以工作区、SQLite 全部落到隔离目录。

**场景脚本**：可复现的端到端验证放 `modules/gui-web/packages/web-console/tools/*_scenario_e2e.py`，
对运行中的 app 打真实 HTTP。随 PR 提交，评审者可自己跑。
参考 `goal_human_ack_scenario_e2e.py`（56 项检查，覆盖正向链路与边界）。

## 4. 依赖链 PR

一个里程碑里的任务通常线性依赖。做法是分支逐个堆叠（GL-0N 基于 GL-0(N-1)），
PR 的 base 仍设 `main`。这样每个 PR 会包含前序 commit，**前序合并后 GitHub 会自动收敛**该 PR 的 diff。

在 PR 描述里写明依赖关系与"只看最后一个 commit"，避免评审者被前序改动干扰。
