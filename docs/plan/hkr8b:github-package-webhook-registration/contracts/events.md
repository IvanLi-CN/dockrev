# 事件（Events）

## github.webhook.package.published（GitHub → Dockrev）

- 范围（Scope）: external
- 变更（Change）: New
- 生产者（Producer）: GitHub Webhooks（GitHub Packages / GHCR）
- 消费者（Consumers）: Dockrev API
- 投递语义（Delivery semantics）: at-least-once；GitHub 可能重试；Dockrev 需按 `X-GitHub-Delivery` 去重（短期 TTL 窗口）

### 触发条件（Trigger）

- `X-GitHub-Event=package`
- payload `action=published`

### 载荷（Payload）

- Schema: GitHub `package` event payload（字段以 GitHub Docs 为准）
- Validation:
  - 允许 `package.repository` 缺失（例如 package 未绑定到 repo）：
    - 若能提取 repo：按 repo 过滤与触发。
    - 若无法提取 repo：按 owner 过滤；命中则触发一次全局 discovery（并记录“repo unknown”告警）。
  - 仅处理 container package（GHCR）；其它 package_type 可忽略并记录告警。

### 安全（Security）

- 必须校验 `X-Hub-Signature-256`（HMAC-SHA256），secret 由服务端生成并在创建 webhook 时写入 GitHub config。

### 兼容性规则（Compatibility rules）

- Additive changes：允许 GitHub 增加字段；Dockrev 使用“容错解析”。
- Deprecations/removals：若关键字段变更导致无法解析 owner/repo，则降级为忽略并暴露可诊断日志。
