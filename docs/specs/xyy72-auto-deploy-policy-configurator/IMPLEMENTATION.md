# Dockrev：自动部署策略配置器 实现状态（#xyy72）

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实，避免这些细节散落到 PR / Git 历史里。

## Current Status

- Implementation: 已实现
- Lifecycle: ready-for-pr
- Catalog note: fast-track（auto policy model + Service/Stack UI + Storybook visual evidence）

## Coverage / rollout summary

- 已实现 Stack/Service 自动更新策略持久化、校验、匹配、继承优先级与 pending 审计。
- 已实现 semver、regex、glob 匹配；延迟规则按首次发现时间与版本滞后数量两个门槛叠加判断。
- 已实现定时检查与 GHCR webhook 检查完成后的自动入队；UI 手动扫描不会触发自动部署；忽略/归档服务与 Dockrev 自身镜像不会被自动入队。
- 已实现 Service Detail 与 Stack Detail 的最终策略结果摘要、最近三次更新记录，以及独立响应式自动更新策略抽屉；桌面从右侧出现，移动端从底部出现。Services/Operations 更新候选列表不再展示 Stack 策略快捷按钮，Service Detail 顶部保留 Stack 详情入口。
- 已把 Service Detail / Stack Detail 共用的最近三次更新记录升级为可访问的行级导航入口；每条记录支持 click、`Enter`、`Space` 直达 `/queue/:jobId`，且行内 `TaskResultReason` 继续独立打开气泡，不会误触发行级跳转。
- 已实现非线性滑块、规则预览、历史版本命中预览、抽屉 Storybook 覆盖与视觉证据落盘；PR 复用截图仍需主人明确提交授权。

## Remaining Gaps

- GitHub PR 创建与远端 push 需等待截图提交授权与可用 GitHub MCP。

## Related Changes

- Backend: auto update policy schema, DB accessors, validators, matching engine, delayed pending queue, schedule/webhook enqueue path.
- API: Stack/Service settings roundtrip and explicit `targets[]` update job audit.
- Web: Service/Stack independent policy editor drawer, Service protection settings drawer, history match preview, detail summary cards, recent update records, Stack detail page, Service Detail stack navigation, client types, mock API, Storybook states.
- Docs: canonical spec status and Storybook visual evidence.

## References

- `./SPEC.md`
- `./HISTORY.md`
