# Dockrev：预期成功工作流失败通知

## 状态

- Status: 已完成
- Lifecycle: active
- 本主题冻结预期成功 workflow 的失败通知路由、上下文边界和验收标准。

## 背景 / 问题陈述

Dockrev 原有 `Notify failed release` 只监听 `Release`。这能覆盖版本、artifact、tag 和 same-SHA recovery，但 `Docs Pages`、`CI (PR)`、`Label Gate`、`Release completion`、`Release Preparation` 与 `Review Policy` 失败时没有统一通知路径。维护者只能主动查看 Actions，无法及时知道文档发布、PR 门禁或交付前置步骤已经失败。

这里需要区分两类通知：`Release` 失败必须保留 release identity、artifact names 和恢复语义；其他预期成功 workflow 只需要通用的 run 上下文，不得伪造版本 identity。通知自身不属于预期成功业务 workflow，必须排除，避免失败通知递归。

## 目标 / 非目标

### Goals

- 对 checked-in allowlist 中所有预期成功 workflow 的 `workflow_run` `failure` 发送通知。
- `Release` 继续使用 `Notify failed release`，保留已验证 release identity 和 same-SHA recovery context。
- 其他当前预期成功 workflow 使用 `Notify failed workflow`，至少携带 workflow、event、ref、branch、head SHA、PR、run ID、run attempt、actor 和 run URL。
- 用仓库内配置声明 expected-success workflow、通知路由和排除项，并由 contract test 保证 allowlist 与实际 workflow 名称不漂移。
- 通知 helper 只从默认分支读取 trusted source，不 checkout 失败 workflow 的 caller branch/code。
- 保留 `workflow_dispatch` smoke path；真实外部通知 transport 仍由既有 Oidrune/OIDC 合同承载。

### Non-goals

- 不把普通 workflow failure 转换成 Release identity、版本或 tag recovery。
- 不改变任何 workflow 的业务步骤、并发、权限或 artifact 生产逻辑。
- 不通知 `skipped`、`cancelled` 或成功运行；本主题当前只定义 GitHub `conclusion == failure`。
- 不在本仓库内修改 Oidrune gateway、OIDC subject/audience allowlist 或外部 delivery channel。

## 接口契约（Interfaces & Contracts）

| 接口 | 类型 | 变更 | 说明 |
| --- | --- | --- | --- |
| `.github/release-failure-notification.json` | checked-in policy | Modify | 声明 expected-success workflow、通用/Release 路由和排除项 |
| `Notify failed workflow` | workflow sidecar | New | 监听非 Release expected-success workflow 的失败并发送通用通知 |
| `workflow_failure_context.py` | trusted helper | New | 只用 `workflow_run` metadata 渲染通用摘要 |
| `Notify failed release` | workflow sidecar | Preserve | 继续校验 Release identity-specific failure context |
| Oidrune `notify.yml` | reusable workflow | Existing | 通过 pinned ref 和 OIDC 发送 summary |

## 规范要求

### REQ-WORKFLOW-FAILURE-001：预期成功 allowlist

- `expected_success_workflows` 必须列出所有当前预期成功的业务/交付 workflow。
- `excluded_workflows` 必须至少包含 `Notify failed release` 与 `Notify failed workflow`。
- contract test 必须断言 allowlist 与 `.github/workflows/*.yml` 的 top-level `name` 完全覆盖，新增 workflow 未分类时必须失败。

### REQ-WORKFLOW-FAILURE-002：失败路由

- 非 `Release` 的 expected-success workflow 在 `completed` 且 `conclusion == failure` 时必须进入 `Notify failed workflow`。
- `Release` 只能进入 `Notify failed release`，不得被通用 sidecar 重复通知。
- 通知 sidecar 自身的失败不应重新触发通知。

### REQ-WORKFLOW-FAILURE-003：通用失败上下文

通用通知摘要必须包含：

- repository、workflow、event、ref/branch；
- head SHA、PR number（如有）、run attempt、actor；
- 可直接打开的 run URL；
- 不把普通 workflow 描述成版本发布或 same-SHA recovery，仅提供检查失败 job 后再重跑的建议。

### REQ-WORKFLOW-FAILURE-004：可信执行边界

- 通用 sidecar 只能 checkout repository default branch 的 helper。
- 不得 checkout 或执行失败 workflow 的 PR head/caller code。
- Oidrune caller 必须使用已批准的 pinned ref 和 `id-token: write`；不得新增 secret 或 gateway override。

## 验收标准

- Given `Docs Pages`、`CI (PR)`、`Label Gate`、`Release completion`、`Release Preparation` 或 `Review Policy` 失败，When `Notify failed workflow` 收到 `workflow_run`，Then 通过 Oidrune 发送通用失败摘要。
- Given `Release` 失败，When `Notify failed release` 收到 `workflow_run`，Then 只发送带 release intent、artifact names、run attempt 和 same-SHA recovery 的专用摘要。
- Given 任一通知 workflow 失败，When workflow_run 事件产生，Then 不会因为自身名称在 allowlist 中而递归触发。
- Given 一个新增 workflow 未加入 expected 或 excluded 分类，When contract test 运行，Then test 失败并阻止 CI 通过。
- Given `workflow_dispatch` 运行任一通知 workflow，Then 只调用 Oidrune smoke path，不启动 Release、Pages 或其他业务 workflow。

## 质量门槛 / 验证

- `python3 .github/scripts/test_workflow_failure_notification_contract.py`
- `python3 .github/scripts/test_docs_pages_workflow_contract.py`
- `bash .github/scripts/release-channel-contract-check.sh`
- `actionlint .github/workflows/*.yml`
- `git diff --check`
- 通过 `spec_drift_check.sh` 验证本主题的 implementation-to-Spec traceability。

## Related ADRs

None

## 实现里程碑

- [x] M1：将 workflow expected-success allowlist、路由和排除项写入 checked-in policy。
- [x] M2：新增通用 `workflow_run` failure sidecar 和 trusted summary helper。
- [x] M3：保留 Release 专用通知，加入 generic notification contract test 和 PR CI gate。
- [x] M4：同步 release 文档、Spec 索引和本主题实现/历史文件。
