# Dockrev 三态主题偏好与响应式入口 实现状态

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实，避免这些细节散落到 PR / Git 历史里。

## Current Status

- Implementation: 已实现
- Lifecycle: active
- Catalog note: fast-track；桌面侧栏、移动设置区、系统同步和 Storybook 证据在同一 PR 收敛

## Coverage / rollout summary

- 主题控制器已提供 `system | light | dark` 偏好、系统解析、storage/media 同步、同页订阅、根节点元数据和显式值迁移。
- `ThemePreferenceControl` 已接入 AppShell：桌面折叠图标、桌面展开滑块、移动设置路由图标；Radix ContextMenu 提供直接 radio 选择。
- 解析主题切换使用从触发控件发出的全视口圆形揭示层；圆心取触发控件中心，终点半径精确计算到视口最远角。目标主题先应用到只读界面副本，揭示动画持续 `1200ms`，覆盖全部视口后才提交真实根主题并移除副本；reduced-motion 环境直接应用最终状态。
- 主题存储与现有 Supervisor 同源合同兼容：缺失 key 表示 system，显式 light/dark 保持原值。
- 视觉证据目标源为 ui_demo（页面/AppShell）与 Storybook docs（可复用主题控件）。

## Validation

- `bun test`：168 项通过。
- `bun run build`：通过。
- `bun run lint`：通过，保留仓库已有 2 条 TanStack Virtual 编译跳过 warning。
- `bun run build-storybook`：通过。
- `DOCKREV_TEST_STORYBOOK_SMOKE_ONLY=1 bun run test-storybook`：332 个故事与交互 smoke 通过。
- `bun run test-storybook`：故事 smoke 通过；既有 queue 深链交互在等待 `#/queue/job-ui-*` 时超时，与主题控件改动无关。
- `cargo test -p dockrev-supervisor`：58 项通过。
- Impeccable detector：无阻断项。

## Remaining Gaps

- 无；PR 仍需按 fast-track 完成远端 CI/review 收敛，但本地实现与证据已闭环。

## Related Changes

- None

## References

- `./SPEC.md`
- `./HISTORY.md`
