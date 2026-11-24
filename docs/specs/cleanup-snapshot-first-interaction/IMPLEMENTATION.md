# Cleanup 快照优先与即时确认弹窗 实现状态

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实。

## Current Status

- Implementation: 未开始
- Lifecycle: active
- Catalog note: 需求已冻结，等待实现

## Implementation Coverage

- Requirement coverage:
  - `REQ-CSI-001`、`REQ-CSI-005`、`REQ-CSI-006`、`REQ-CSI-009`、`REQ-CSI-015`：`web/src/pages/CleanupPage.tsx` 的加载、过期、调度与自动刷新路径。
  - `REQ-CSI-002`、`REQ-CSI-003`、`REQ-CSI-004`、`REQ-CSI-008`、`REQ-CSI-010`、`REQ-CSI-011`、`REQ-CSI-012`：确认弹窗组件、发现态视图、Storybook 场景。
  - `REQ-CSI-007`、`REQ-CSI-013`、`REQ-CSI-014`：`crates/dockrev-api/src/api/cleanup_routes.rs`、`crates/dockrev-api/src/cleanup_snapshot_worker.rs`、`crates/dockrev-api/src/api/types/cleanup.rs`。
- Verification commands:
  - `cargo test -p dockrev-api cleanup -- --nocapture`
  - `bun run --cwd web lint`
  - `bun run --cwd web build`
  - `bun run --cwd web build-storybook`
  - `bun run --cwd web test-storybook -- --url <leased-port>`
- Rollout facts: 服务端新增响应字段为向后兼容增量；scan-run 的 `page + aggressive + all` 限制保持不变。

## Coverage / rollout summary

- 尚未开始实现，未记录 rollout 事实。

## Remaining Gaps

- 全部需求待实现。
- 需要验证 `snapshot_gate` 从整轮扫描互斥收窄为删除互斥时，Stack 删除与扫描最终写入的交错仍然安全。

## Related Changes

- ADR: [`docs/adr/0023-cleanup-reads-existing-snapshot.md`](../../adr/0023-cleanup-reads-existing-snapshot.md)

## References

- `./SPEC.md`
- `./HISTORY.md`
