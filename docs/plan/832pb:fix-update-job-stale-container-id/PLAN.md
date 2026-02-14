# Dockrev API: 更新任务使用 stale container id 导致误报失败（#832pb）

## 状态

- Status: 待实现
- Created: 2026-02-14
- Last: 2026-02-14

## 背景 / 问题陈述

- 线上更新队列中出现大量 `update` job 标记为 `failed`，但日志显示 `docker-compose ... up -d <service>` 实际已成功完成。
- 失败点通常发生在 `up -d` 之后的 `docker inspect --format ... <container_id>`：当 `up -d` 触发 `Recreate`，此前通过 `docker-compose ps -q` 拿到的 container id 可能已被移除。
- 在启用 `docker-socket-proxy` 的环境中，旧 container id 不存在时，`docker inspect` 可能 fallback 到 inspect 其它对象类型，从而命中被 proxy 禁止的 endpoint，返回 `403 Forbidden`，导致整条 job 被标记为失败（误报）。

## 目标 / 非目标

### Goals

- 更新/回滚路径在 `up -d` 之后，使用 **最新** container id 进行 healthcheck 判断与 new digest 记录，避免对已删除容器做 inspect。
- 补齐单元测试，覆盖 “container id 发生变化” 的场景，防止回归。
- 本地自测通过，并在共享测试机（`codex-testbox`）做一次模拟测试验证通过。

### Non-goals

- 不调整 `docker-socket-proxy` 的 ACL 规则。
- 不改动 Web UI 展示逻辑。
- 不重做健康检查策略（仅修复 container id 选择/刷新逻辑）。

## 范围（Scope）

### In scope

- `crates/dockrev-api` 更新任务（update job）在 `pull + up` 后刷新 container id，再执行 inspect/healthcheck/digest 记录。
- 回滚（rollback）路径同样刷新 container id 后再执行健康检查与 digest 记录。
- 新增/调整单元测试（最小覆盖）。

### Out of scope

- 备份 job 与 discovery/check 任务逻辑。
- compose 文件生成逻辑与候选版本计算逻辑。

## 需求（Requirements）

### MUST

- `up -d` 后必须再次 `ps -q <service>` 获取 container id，并用于：
  - 判断是否存在 healthcheck
  - 等待 healthy（如适用）
  - 记录 new image digest
- rollback 后同样刷新 container id（因为 rollback 的 `up` 也可能 Recreate）。
- 单元测试覆盖 “up 后 container id 变化” 的路径，并验证 inspect/healthcheck 使用新 id。

### SHOULD

- 失败/空容器 id 时的错误信息更明确（便于排障）。

### COULD

- 未来考虑对 `ps -q` 多行输出做显式处理（当前保持既有行为）。

## 接口契约（Interfaces & Contracts）

None

## 验收标准（Acceptance Criteria）

- Given `docker-compose up -d` 导致容器 Recreate（container id 变化）
  When Dockrev 执行 `update` job
  Then 后续 `docker inspect` / healthcheck / new digest 记录使用 **更新后的** container id，不再对已删除容器做 inspect，job 不再误报失败。
- 单元测试通过，且在共享测试机（`codex-testbox`）模拟运行验证通过。

## 实现前置条件（Definition of Ready / Preconditions）

- 本计划不涉及接口契约变更（`None`）
- 验收标准与测试策略已冻结

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests: 覆盖 update job 在 `up -d` 后刷新 container id 的行为
- Integration tests: None（本次以模拟 runner + 共享测试机验证为主）

### Quality checks

- `cargo test -p dockrev-api` 通过

## 文档更新（Docs to Update）

- None（除本计划文档本身）

## 计划资产（Plan assets）

- None

## 资产晋升（Asset promotion）

None

## 实现里程碑（Milestones）

- [ ] M1: 修复 update job（up/rollback 后刷新 container id）
- [ ] M2: 新增/调整单元测试覆盖 container id 变化场景
- [ ] M3: 本地与 `codex-testbox` 模拟测试通过；PR checks 通过

## 方案概述（Approach, high-level）

- 在每个 service 的更新流程中：
  - `ps -q` 获取更新前 container id，用于记录 old image digest
  - `pull` + `up -d`
  - 再次 `ps -q` 获取更新后 container id，后续 healthcheck 与 new digest 全部使用此 id
- rollback 分支在 `up -d --pull never` 后同理刷新 container id 再等待 healthy/记录 digest

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：`docker-compose ps -q <service>` 可能返回多行（scale > 1），目前实现仅取 trim 后的整体字符串；本次修复保持既有行为。
- 假设：一次更新 job 仅关注单容器实例（与现有行为一致）。

## 变更记录（Change log）

- 2026-02-14: 初版冻结

## 参考（References）

- 线上队列日志：`update` job 在 `up -d` 成功后仍因 `docker inspect` 失败而标记 `failed`

