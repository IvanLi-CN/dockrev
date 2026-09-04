# Dockrev 统一页面内导航实现状态

## Current Status

- Implementation: complete
- Lifecycle: active
- Delivery flow: fast-track；目标为直接 PR 的 `Step 5C Ready`，不执行合并。

## Coverage

- AppShell 使用单一 context-navigation 插槽；桌面侧栏、移动底部一级导航和移动 context drawer 按断点互斥挂载。
- Overview、Queue、Services/Stack/Service、Cleanup、Settings 的页面内导航复用已有读模型；清理筛选保持纯视图语义。
- PageHarness、Storybook stories 和页面内导航单测覆盖布局与交互合同。

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
