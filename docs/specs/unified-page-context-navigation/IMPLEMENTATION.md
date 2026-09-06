# Dockrev 统一页面内导航实现状态

## Current Status

- Implementation: in progress
- Lifecycle: active
- Delivery flow: fast-track；当前进入 PR 收敛，目标为 merge-ready PR；本次不执行合并。

## Coverage

- AppShell 使用单一 context-navigation 插槽；桌面侧栏、移动底部一级导航和移动 context drawer 按断点互斥挂载，移动抽屉底部补齐③身份、主题与版本元信息。
- Overview、Queue、Services/Stack/Service、Cleanup、Settings 的页面内导航复用已有读模型；清理筛选保持纯视图语义。
- Overview 宽屏页头按资源摘要、浏览器本地当前时间和服务搜索排列；时钟由浏览器本地每秒刷新并显示 GMT 偏移，与扫描和资源样本时间分离。
- Overview 受限宽屏保留资源摘要，以图标触发搜索弹层并卸载时钟；图标本身不提交或更改筛选。窄屏时资源摘要、时钟和唯一搜索输入只挂载于 context drawer，正文、侧栏和页头不保留副本。
- 桌面侧栏移除一级图标前的“导航”标题；Overview context navigation 在桌面只承担分组定位。
- Settings ② 仅显示区块名称与当前态，区块描述留在主内容区，避免侧栏长文案截断。
- PageHarness、Storybook stories 和页面内导航单测覆盖完整宽屏、受限宽屏搜索弹层、窄屏 context drawer，以及单一搜索输入和时钟归属合同。
- 适用的 mock-only 视觉证据比较会在当前 Overview 呈现状态完成后更新；移动抽屉保留③身份、主题与版本元信息。

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
