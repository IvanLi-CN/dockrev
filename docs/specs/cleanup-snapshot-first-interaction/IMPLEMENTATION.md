# Cleanup 快照优先与即时确认弹窗 实现状态

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实。

## Current Status

- Implementation: 已实现，待 PR
- Lifecycle: active
- Catalog note: 服务端合同、前端弹窗、Storybook 证据与验证已完成

## Implementation Coverage

- `REQ-CSI-001`、`REQ-CSI-005`、`REQ-CSI-006`、`REQ-CSI-009`、`REQ-CSI-015`：`web/src/pages/CleanupPage.tsx` 的快照优先加载、过期标记、前台刷新调度、退避与静默更新。
- `REQ-CSI-002`、`REQ-CSI-003`、`REQ-CSI-004`、`REQ-CSI-008`、`REQ-CSI-010`、`REQ-CSI-011`、`REQ-CSI-012`：`web/src/pages/CleanupConfirmDialog.tsx` 与 `web/src/App.css` 的即时弹窗、发现态、阶段动效、失败/过期切换与 reduced-motion 降级。
- `REQ-CSI-007`、`REQ-CSI-013`、`REQ-CSI-014`：`crates/dockrev-api/src/api/cleanup_routes.rs`、`crates/dockrev-api/src/cleanup_snapshot_worker.rs`、`crates/dockrev-api/src/cleanup.rs` 的 apply 准入、扫描单飞、删除互斥、`snapshotAgeMs` 与有界 partial 节奏。
- 客户端按目标 stack/service 的投影在 `cleanupPageModel.ts`；快照年龄、提前量与退避的纯函数有单元测试覆盖。

## Verification

- `cargo test -p dockrev-api cleanup_`：38 passed。
- `cargo fmt --check`：通过。
- `bun test`（web）：274 passed。
- `bun run --cwd web lint`：0 errors（仅既有 warning）。
- `bun run --cwd web build`：通过。
- `bun run --cwd web build-storybook`：435 stories。
- `DOCKREV_TEST_STORYBOOK_SMOKE_ONLY=1 bun run --cwd web test-storybook`：All stories passed。
- `DOCKREV_TEST_STORYBOOK_INTERACTIVE_ONLY=1 bun run --cwd web test-storybook`：通过（含 cleanup 窄屏确认布局）。
- `bun run --cwd web storybook:screenshots -- --only pages-cleanuppage--discovery-state,...`：生成 `./assets/cleanup-*.png`。

## Coverage / rollout summary

- 页面读取新鲜快照时不再隐式启动扫描；动作弹窗使用内存快照立即打开。
- 动作确认的 scoped fingerprint 仍由服务端计算；apply 在扫描运行中仍可用，但继续校验 300 秒新鲜度、fingerprint 与 volume 身份。
- 删除门闩由整轮扫描互斥改为读写门闩，扫描另加单飞锁；Stack 记录删除仍与快照最终写入和 apply 互斥。

## Remaining Gaps

- PR 尚未创建；发布前需按项目流程补 PR 与最终视觉确认。
- 若真实环境的单轮扫描耗时接近或超过 240 秒，自适应提前量只能减轻等待，不能保证快照始终新鲜；并行化扫描不在本主题范围内。

## Related Changes

- ADR: [`docs/adr/0023-cleanup-reads-existing-snapshot.md`](../../adr/0023-cleanup-reads-existing-snapshot.md)

## References

- `./SPEC.md`
- `./HISTORY.md`
