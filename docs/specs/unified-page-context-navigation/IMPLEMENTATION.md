# Dockrev 统一页面内导航实现状态

## Current Status

- Implementation: complete
- Lifecycle: active
- Delivery flow: fast-track；当前进入 `merge-only`，目标为直接 PR 合并并完成合并后验证。

## Coverage

- AppShell 使用单一 context-navigation 插槽；桌面侧栏、移动底部一级导航和移动 context drawer 按断点互斥挂载，移动抽屉底部补齐③身份、主题与版本元信息。
- Overview、Queue、Services/Stack/Service、Cleanup、Settings 的页面内导航复用已有读模型；清理筛选保持纯视图语义。
- Overview 服务搜索归属②：桌面只在侧栏渲染，移动只在抽屉渲染，页头不再重复渲染搜索控件。
- Settings ② 仅显示区块名称与当前态，区块描述留在主内容区，避免侧栏长文案截断。
- PageHarness、Storybook stories 和页面内导航单测覆盖布局与交互合同。
- 已确认并落盘五个页面桌面/移动共 10 张 mock-only 视觉证据；移动抽屉包含③身份、主题与版本元信息。

## Validation

- `python3 bin/spec_contract_check.py --path docs/specs/unified-page-context-navigation/SPEC.md`
- `bash skills/spec-sync/scripts/spec_drift_check.sh`
- `bun run --cwd web lint`
- `bun run --cwd web test`
- `bun run --cwd web build`
- `bun run --cwd web build-storybook`
- `bun run --cwd web test-storybook`

## References

- `./SPEC.md`
- `./HISTORY.md`
