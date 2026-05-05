# Implementation: 更新确认弹窗信息收敛（#ttfyf）

## 当前实现状态

- Status: 已实现
- Last: 2026-05-05

## 实现覆盖

- 新增单服务更新确认详情组件，承载完整 service update 决策信息。
- 导航页服务卡片更新弹窗改用单服务确认详情组件。
- 服务详情页与 Operations/Services 服务级更新确认正文改用同一单服务确认详情组件。
- 聚合更新预览继续使用独立列表组件，不承载单服务完整详情。
- 弹窗内镜像与版本信息改用专用扁平堆叠容器，去掉无意义的内缩进。
- 目标 digest 只显示一次，但必须直接展示完整值，允许自然换行以支持人工校验。
- 单服务确认正文移除 `范围 service`、标题已表达的服务名与默认 `备份 inherit`，减少重复字段；提交请求仍保留 `scope=service` 与 `backupMode=inherit`。
- 单服务确认正文中的 `状态` 与 `架构策略` 值改为语义 badge，保留原始值文本并提升扫读辨识度。
- 状态 badge 按状态语义映射 action/warn/bad/neutral，并允许长状态文本在窄屏换行，避免未知状态或长文本破坏弹窗布局。

## 验证

- `bun --cwd web lint`
- `bun --cwd web build`
- `bun --cwd web build-storybook -- --quiet`
- `bun --cwd web test-storybook`
- 视觉证据已写入 `SPEC.md` 的 `## Visual Evidence`：
  - `assets/overview-service-update-confirm-desktop.png`
  - `assets/service-update-confirm-mobile.png`
  - `assets/service-update-confirm-hint-badge.png`
  - `assets/service-update-confirm-long-status-narrow.png`
  - `assets/service-update-confirm-badge-light.png`
