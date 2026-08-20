# Dockrev 异步数据连续性与加载反馈

> 当前有效规范以本文为准；实现覆盖与当前状态见 `./IMPLEMENTATION.md`，关键演进原因见 `./HISTORY.md`。

## 背景 / 问题陈述

- 多个正式路由把尚未完成的异步读取渲染为离线、空数据或真实的 `0`，使操作员在数据仍在到达时得到错误结论。
- 自动同步曾用与用户操作相同的磨砂遮罩覆盖已提交内容；当候选列表执行逐 Stack 详情读取时，整张长表会长时间不可读。
- 现有 PWA 只读快照可在短时断连和后台刷新期间保持连续性，但 payload 没有表达数据域就绪度，时间窗图表也可能展示已经越窗的数据。

## 目标 / 非目标

### Goals

- 建立跨页面共享的异步数据相位、数据来源和区域状态合同。
- 首次无数据加载时显示贴合真实布局的骨架；已有最后成功数据时保留内容，只有用户显式读取才显示区域级加载遮罩。
- 原地隔离失败并提供重试，不让其他成功区域失去可用性。
- 依数据域管理 fresh 只读快照，并使时间窗资源数据以当前时间为锚点裁剪。
- 覆盖所有正式异步路由，并用 mock `ui_demo`、Storybook 和自动化回归保证状态真实。

### Non-goals

- 不修改数据库、鉴权模型、Service Worker 业务缓存范围或敏感数据持久化范围。
- 不持久化设置、密钥、日志、GHCR delivery、认证态或写操作上下文。
- 不将普通请求失败误称为离线，也不重做信息架构、配色或业务操作流程。

## 范围（Scope）

### In scope

- `AsyncDataPhase`、请求意图、新鲜度策略、`AsyncDataRegion` 与布局骨架原语。
- 只读的批量候选摘要 `GET /api/stacks/overview` 及候选表虚拟渲染。
- 首页、服务大盘、Queue、版本推测、Stack、Service 各只读页、Job Detail、系统设置、部署检查、GHCR 三页和壳层服务树。
- fresh snapshot v2、资源时间窗裁剪、mock 路由延迟/失败行为、Storybook 状态矩阵和 `ui_demo` 场景。

### Out of scope

- 写操作自身的任务进度和持续 SSE 重连语义；它们保持非阻断状态提示。
- 生产站点写入、发布或真实程序截图。

## 需求（Requirements）

### MUST

- 相位值域固定为 `initial-loading | ready-empty | ready-data | refreshing | error | offline`；来源值域固定为 `none | live | memory | fresh-snapshot`。
- `initial-loading` 且无数据时展示骨架；`ready-empty` 只能在成功响应确认为空后展示；`offline` 只能在 `isOnline === false` 时展示。
- 请求意图值域固定为 `initial | user-action | background`。用户主动刷新、翻页、筛选、搜索提交和重试在请求开始时令相应触发控件 busy/disabled，并在 200ms 后显示遮罩；自动事件、轮询、SSE 重连与有效快照的后台读取绝不挂遮罩、绝不设 `aria-busy`、绝不拦截指针或禁用已有控件。
- 自动同步持续超过 5 秒时，只能在受影响区域的可见视口内显示无布局位移的被动提示；成功后保留 1.5 秒再淡出。自动失败保留最后成功数据并显示非阻断重试，不得覆盖内容。
- 用户加载遮罩保留最后成功内容，具有 `aria-busy` 与 `role=status`；用户读取失败立即显示具有 `role=alert` 与重试按钮的错误遮罩。无数据、过期数据的在线读取失败以骨架为背景。
- 查询标签、分页和结果只在成功时原子提交；较晚返回的旧请求必须丢弃。
- 多数据域页面独立收敛：失败区域保留最后成功内容并可重试，其他区域继续可用。
- 新鲜度档位固定为：任务/运行态/监控 `15s`，候选/队列/版本推测/GHCR/发现 `60s`，设置/备份/归档 `300s`。快照 payload 必须含版本、数据域 readiness、已提交查询键和提交时间；旧的歧义 payload 不得展示，并在下一次成功读取后自然替换。
- 所有 GET 必须使用 15 秒 `AbortController` deadline。改变 query 的用户操作取消旧后台读取，结果、页码和查询标签只在同一成功提交中更新；远端更新不得覆盖 dirty 表单字段，而应暂存到取消编辑或保存成功后的协调路径。
- 资源样本的 cutoff 为 `Date.now() - selectedWindow`；无效、未来或越窗样本必须排除。裁剪后无可用缓存时，在线显示骨架、离线显示真实离线态。
- 遮罩继承区域圆角，使用主题 token 和约 68% panel 背景、`blur(6px) saturate(115%)`、160ms 淡出。减少动效下停止扫光、旋转和过渡，但保留语义文案。

### SHOULD

- 骨架复用首页的扫光语言并按真实布局组合，保持稳定尺寸，避免布局跳动。
- 持续 SSE 重连只显示既有非阻断提示，不反复覆盖内容。
- 缓存只读来源期间，依赖该数据的写操作保持禁用，直至 live 成功提交。

## 接口契约（Interfaces & Contracts）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | Owner | 使用方 | 备注 |
| --- | --- | --- | --- | --- | --- | --- |
| `AsyncDataPhase` | TypeScript type | internal | New | web UI | all async regions | 明确加载、空、错误和离线真相 |
| `AsyncDataSource` | TypeScript type | internal | New | web UI | snapshot-aware pages | 表达 live、memory、fresh snapshot 来源 |
| `AsyncDataRequestIntent` | TypeScript type | internal | Modify | web UI | all async regions | 区分首次、用户读取和完全非阻断后台读取 |
| `AsyncFreshnessProfile` | TypeScript type | internal | New | web UI | snapshot-aware pages | 固定 15/60/300 秒数据域策略 |
| `AsyncDataRegion` | React fragment | internal | Modify | web UI | page data domains | 用户遮罩、后台轻提示、可访问性和重试 |
| readonly snapshot v2 | IndexedDB payload | internal | Modify | web PWA | snapshot-aware pages | 含版本、readiness、committed query key |
| `GET /api/stacks/overview` | HTTP read model | API | New | API | operations dashboard | 一次返回候选表所需的 Stack 与服务摘要 |
| `dockrevApiBehaviorByRoute` | mock API contract | test-only | New | web stories/demo | deterministic UI tests | 用 `METHOD pathname` 配置 delay 与失败 |

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 冷启动：没有 live、memory 或 fresh snapshot 数据时立即显示相应布局骨架；成功后原子进入 `ready-data` 或 `ready-empty`。
- 用户读取：保持最后成功数据，超过 200ms 后叠加半透明磨砂加载层；完成即以 160ms 淡出。
- 自动同步：保持最后成功数据和所有控件可用；超过 5 秒仅显示区域内浮动同步提示，成功后 1.5 秒淡出。失败时显示非阻断错误与重试。
- 失效恢复：焦点、前台和网络恢复仅在对应数据域超出其新鲜度时进入骨架加载；在线读取失败则显示骨架加错误遮罩。重试路径为 `error -> user-action -> ready-*`。
- 快照恢复：仅 v2 且 fresh 的只读数据可先渲染；后台 live 成功后提交真实数据并解除只读限制。
- 并发查询：每次请求有最新身份；晚到结果不覆盖新查询、分页或筛选。
- 候选列表：首屏与后台刷新只读取批量摘要；前端使用虚拟滚动，完整 Stack 详情仅在展开组或执行操作时读取，单组失败不影响其余候选。

### Edge cases / errors

- 网络在线时的 HTTP、解析或局部域失败是 `error`，绝不显示 offline。
- 同一页面部分域成功、部分域失败时，仅失败域呈现与请求意图相符的恢复反馈；后台失败不得覆盖成功内容。
- 资源缓存裁剪后为空不允许呈现伪零值图表或“暂无数据”成功文案。
- `prefers-reduced-motion: reduce` 不得依赖动画表达 busy、错误或可重试语义。
- 自动更新命中 dirty 表单字段时，本地草稿优先；保存成功后重取服务端结果，取消编辑后应用暂存远端快照。

## 验收标准（Acceptance Criteria）

- Given 初次进入任一异步路由，When 数据尚未完成，Then 只显示相应骨架，且不显示离线、空态或真实零值。
- Given 有已提交数据并主动刷新，When 请求超过 200ms，Then 保留内容并显示区域加载遮罩；快速完成时不显示遮罩。
- Given 有已提交数据的自动同步，When 请求超过 5 秒，Then 内容和控件仍可读可用，只显示区域内被动同步提示。
- Given 自动同步失败，When 网络仍在线，Then 保留最后成功内容并显示非阻断重试；无有效数据时才显示骨架加错误遮罩。
- Given snapshot 过期、旧版或裁剪后无有效时间窗样本，When 页面恢复，Then 不展示该缓存内容。
- Given 分页或筛选快速连续变化，When 较旧请求后返回，Then 标签与结果仍对应最新成功查询。
- Given 暗/亮、桌面/移动和减少动效环境，When 检视 async region，Then 无布局跳动，文字、焦点和状态语义保持可用。

## 非功能性验收 / 质量门槛（Quality Gates）

- Tests: `cargo test -p dockrev-api`, `bun test`, `bun run lint`, `bun run build`, `bun run build-storybook`, `bun run test-storybook`, `bun run build:demo:pages`。
- Storybook: `AsyncDataRegion` 状态 gallery 与用户遮罩、后台轻提示、超时和非阻断失败交互。
- Visual evidence: mock-only `ui_demo` 的自动慢同步、自动失败、用户读取，桌面 `1440x900` 与移动 `393x852`，再由主人确认。

## 风险 / 假设

- 假设：snapshot 仅承载既有只读 read model；其有效期依数据域的 15/60/300 秒档位判断。
- 风险：状态域拆分不完整会继续让局部失败污染整页；所有 catch 分支必须明确进入 error 或真实 offline。
- 风险：查询提交如果混合 pending/committed 状态会造成标签与列表错配；分页和筛选需要同步提交。

## 参考

- `docs/specs/r8kpa-web-pwa-offline-shell/SPEC.md`
- `docs/specs/c2r2u-detail-route-service-navigation/SPEC.md`
- `docs/specs/e3f83-ghcr-webhook-inbox-sse/SPEC.md`
- `docs/specs/kdapc-version-inference-decouple/SPEC.md`
