# Dockrev: 候选版本号推断修复（candidate resolved tag）

## 背景

当前服务列表的候选版本主展示位直接使用 `candidate.tag`。当候选 tag 是 `latest` 这类 floating tag 时，即使同 digest 已存在更准确的 semver tag（例如 `v0.2.15`），UI 仍显示 `latest`，导致版本识别不准确。

## 目标

在不改变“同 tag + digest-only 更新”语义的前提下，让候选版本主展示位优先显示可推断的 semver 版本。

## 范围

### In

- 后端在 check 阶段推断并持久化 `candidate.resolvedTag`。
- API 下发 `candidate.resolvedTag`（可选字段，向后兼容）。
- 前端候选版本显示优先使用 `candidate.resolvedTag`。
- 补充后端回归测试与前端场景验证。

### Out

- 不改变候选生成规则（仍是同 tag digest-only）。
- 不改变更新执行与 digest 锁定逻辑。
- 不改 discovery / supervisor / release 流程。

## 验收标准

1. Given 服务候选 `candidate.tag=latest` 且 candidate digest 对应 tags 含 `v0.2.15`，When check 完成并刷新列表，Then 主展示候选版本为 `v0.2.15`。
2. Given candidate digest 无可用 semver tag，When 列表展示，Then 候选版本回退显示原始 `candidate.tag`，不显示空值。
3. Given 运行态回写走降级路径（推断失败），When 完成 runtime scan，Then `candidate.resolvedTag` 被清空，避免陈旧展示。
4. 更新请求行为保持不变：仍使用 candidate digest 锁定，不支持跨 tag 更新。

## 测试

- Rust: 增加/更新 `api/tests.rs` 覆盖 candidate resolved tag 推断与降级清理。
- Web: 更新 Storybook mock 场景，验证列表与确认弹窗候选展示值。
- 最小验证：
  - `cargo test -p dockrev-api resolved_tag_inference`
  - `cargo test -p dockrev-api candidate`
  - `bun --cwd web build`

## 风险

- digest tags snapshot 受扫描深度与 registry 超时影响，推断结果可能不完整。
- 若浮动 tag 语义复杂，可能出现“无 semver 可推断”场景；此时按设计回退原始 tag。

## 里程碑

- [x] M1 后端：`candidate.resolvedTag` 推断 + 持久化 + API 输出 + 回归测试。
- [x] M2 前端：候选版本主展示切换到 resolved 优先 + 场景验证。

## 进展记录

- 2026-02-21: 完成 M1/M2；已补后端回归测试并通过前端构建验证。
