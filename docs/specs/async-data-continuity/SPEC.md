# Dockrev 异步数据连续性与加载反馈

> 当前有效规范以本文为准；实现覆盖与当前状态见 `./IMPLEMENTATION.md`，关键演进原因见 `./HISTORY.md`。

## 背景 / 问题陈述

- 多个正式路由把尚未完成的异步读取渲染为离线、空数据或真实的 `0`，使操作员在数据仍在到达时得到错误结论。
- 已有成功数据时，整块内容被清空或没有刷新反馈；请求失败也经常与无结果或离线状态混淆。
- 现有 PWA 只读快照可在短时断连和后台刷新期间保持连续性，但 payload 没有表达数据域就绪度，时间窗图表也可能展示已经越窗的数据。

## 目标 / 非目标

### Goals

- 建立跨页面共享的异步数据相位、数据来源和区域状态合同。
- 首次无数据加载时显示贴合真实布局的骨架；已有最后成功数据时保留内容，在恰当的延迟后显示区域级加载遮罩。
- 原地隔离失败并提供重试，不让其他成功区域失去可用性。
- 只复用现有 60 秒 fresh 只读快照，并使时间窗资源数据以当前时间为锚点裁剪。
- 覆盖所有正式异步路由，并用 mock `ui_demo`、Storybook 和自动化回归保证状态真实。

### Non-goals

- 不修改后端 API、数据库、鉴权模型、Service Worker 业务缓存范围或 60 秒 fresh 门槛。
- 不持久化设置、密钥、日志、GHCR delivery、认证态或写操作上下文。
- 不将普通请求失败误称为离线，也不重做信息架构、配色或业务操作流程。

## 范围（Scope）

### In scope

- `AsyncDataPhase`、`AsyncDataSource`、`AsyncDataRegion` 与布局骨架原语。
- 首页、服务大盘、Queue、版本推测、Stack、Service 各只读页、Job Detail、系统设置、部署检查、GHCR 三页和壳层服务树。
- fresh snapshot v2、资源时间窗裁剪、mock 路由延迟/失败行为、Storybook 状态矩阵和 `ui_demo` 场景。

### Out of scope

- 写操作自身的任务进度和持续 SSE 重连语义；它们保持非阻断状态提示。
- 生产站点写入、发布或真实程序截图。

## 需求（Requirements）

### MUST

- 相位值域固定为 `initial-loading | ready-empty | ready-data | refreshing | error | offline`；来源值域固定为 `none | live | memory | fresh-snapshot`。
- `initial-loading` 且无数据时展示骨架；`ready-empty` 只能在成功响应确认为空后展示；`offline` 只能在 `isOnline === false` 时展示。
- 用户主动刷新、翻页或筛选在请求开始时立即令触发控件 busy/disabled，并在 200ms 后显示遮罩；fresh 快照或自动后台同步在 800ms 后显示。阈值内完成不得挂载加载遮罩。
- 加载遮罩保留最后成功内容，具有 `aria-busy` 与 `role=status`；错误遮罩立即出现、具有 `role=alert` 与重试按钮。首次失败显示与数据区域等高的独立错误态，不保留骨架底层。
- 查询标签、分页和结果只在成功时原子提交；较晚返回的旧请求必须丢弃。
- 多数据域页面独立收敛：失败区域保留最后成功内容并可重试，其他区域继续可用。
- 只恢复 60 秒内的 v2 fresh snapshot。v2 必须含版本、数据域 readiness 和已提交查询键；旧的歧义 payload 不得展示，并在下一次成功读取后自然替换。
- 资源样本的 cutoff 为 `Date.now() - selectedWindow`；无效、未来或越窗样本必须排除。裁剪后无可用缓存时，在线显示骨架、离线显示真实离线态。
- 遮罩继承区域圆角，使用主题 token 和约 68% panel 背景、`blur(6px) saturate(115%)`、160ms 淡出。减少动效下停止扫光、旋转和过渡，但保留语义文案。
- Job Detail 的首屏快照、管理事件重同步和 SSE 对账读取必须按 `jobId` 合并为一个未完成的自动请求；同一序列只允许最新结果提交。
- Job Detail 快照读取必须在 10 秒后中止；自动读取最多在 1 秒后重试一次，重试仍失败时进入错误态并提供重试入口，不得无限停留在 `initial-loading`。
- Job Detail 无已提交数据的首屏错误必须使用尺寸足够的单色或双色简约插图，而不是简单错误图标；插图不得遮挡错误文案或重试入口。运行时使用 Adobe Spectrum `@spectrum-icons/illustrations` 的 Apache-2.0 `Error` 原始透明 SVG 几何，并通过 Dockrev 主题 token 控制颜色；不得生成、描摹、裁切或为主题维护第二份几何。完整的来源、审查项和对象清单见 [VISUAL_REVIEW.md](./VISUAL_REVIEW.md)。
- Job Detail 的用户刷新必须取消活动自动请求及待执行自动重试并立即替换；卸载或切换 `jobId` 时取消请求、计时器和 SSE。

### SHOULD

- 骨架复用首页的扫光语言并按真实布局组合，保持稳定尺寸，避免布局跳动。
- 持续 SSE 重连只显示既有非阻断提示，不反复覆盖内容。
- 缓存只读来源期间，依赖该数据的写操作保持禁用，直至 live 成功提交。

## 接口契约（Interfaces & Contracts）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | Owner | 使用方 | 备注 |
| --- | --- | --- | --- | --- | --- | --- |
| `AsyncDataPhase` | TypeScript type | internal | New | web UI | all async regions | 明确加载、空、错误和离线真相 |
| `AsyncDataSource` | TypeScript type | internal | New | web UI | snapshot-aware pages | 表达 live、memory、fresh snapshot 来源 |
| `AsyncDataRegion` | React fragment | internal | New | web UI | page data domains | 稳定尺寸、延迟遮罩、可访问性和重试 |
| readonly snapshot v2 | IndexedDB payload | internal | Modify | web PWA | snapshot-aware pages | 含版本、readiness、committed query key |
| `dockrevApiBehaviorByRoute` | mock API contract | test-only | New | web stories/demo | deterministic UI tests | 用 `METHOD pathname` 配置 delay 与失败 |

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 冷启动：没有 live、memory 或 fresh snapshot 数据时立即显示相应布局骨架；成功后原子进入 `ready-data` 或 `ready-empty`。
- 有数据刷新：保持最后成功数据，按来源和触发方式在 200ms 或 800ms 后叠加半透明磨砂加载层；完成即淡出。
- 失败恢复：存在旧数据时在其上显示错误遮罩；不存在旧数据时显示与数据区域等高的独立错误态，不保留骨架底层。重试路径为 `error -> refreshing -> ready-*`。
- Job Detail 首屏：初始快照与首次管理事件重同步共享一个自动请求；请求超时后执行一次延迟重试，最终失败显示可恢复错误而不是永久骨架。
- Job Detail 手动恢复：用户刷新替换任何自动序列；被取消序列的结果和错误不得覆盖替代请求提交的页面状态。
- 快照恢复：仅 v2 且 fresh 的只读数据可先渲染；后台 live 成功后提交真实数据并解除只读限制。
- 并发查询：每次请求有最新身份；晚到结果不覆盖新查询、分页或筛选。

### Edge cases / errors

- 网络在线时的 HTTP、解析或局部域失败是 `error`，绝不显示 offline。
- 同一页面部分域成功、部分域失败时，仅失败域展示错误覆盖层。
- 资源缓存裁剪后为空不允许呈现伪零值图表或“暂无数据”成功文案。
- `prefers-reduced-motion: reduce` 不得依赖动画表达 busy、错误或可重试语义。

## 验收标准（Acceptance Criteria）

- Given 初次进入任一异步路由，When 数据尚未完成，Then 只显示相应骨架，且不显示离线、空态或真实零值。
- Given 有已提交数据并主动刷新，When 请求超过 200ms，Then 保留内容并显示区域加载遮罩；快速完成时不显示遮罩。
- Given fresh snapshot 或后台同步，When 请求超过 800ms，Then 显示加载遮罩；仍可读取现有内容。
- Given 某数据域失败，When 网络仍在线，Then 该域显示错误遮罩和重试，其他成功域保持可用。
- Given 首次快照失败且没有已提交数据，When 错误态出现，Then 数据区域不渲染骨架，错误提示占据并居中于稳定尺寸的错误区域。
- Given snapshot 过期、旧版或裁剪后无有效时间窗样本，When 页面恢复，Then 不展示该缓存内容。
- Given 分页或筛选快速连续变化，When 较旧请求后返回，Then 标签与结果仍对应最新成功查询。
- Given Job Detail 首屏读取与管理事件重同步同时发生，When 任一请求成功，Then 只启动一个快照 GET 并渲染任务详情。
- Given Job Detail 快照读取连续失败，When 首次请求超过 10 秒或一次自动重试仍失败，Then 页面在有界时间内显示错误和重试入口，不继续显示无限骨架。
- Given Job Detail 首屏读取最终失败，When 错误态出现，Then 显示与当前主题对应的透明双色插图、错误文案和重试按钮，而非简单错误图标或骨架。
- Given 用户在自动读取或其重试等待期点击刷新，When 替代读取完成，Then 旧请求结果不会覆盖最新任务状态，且不会再触发 effect 级重复 GET。
- Given 暗/亮、桌面/移动和减少动效环境，When 检视 async region，Then 无布局跳动，文字、焦点和状态语义保持可用。

## 非功能性验收 / 质量门槛（Quality Gates）

- Tests: `bun test`, `bun run lint`, `bun run build`, `bun run build-storybook`, `bun run test-storybook`, `bun run build:demo:pages`。
- Storybook: `AsyncDataRegion` 状态 gallery 与关键交互（延迟阈值、错误重试）；Job Detail 首屏失败到手动恢复的 play 覆盖；错误插图按亮、暗主题切换并保持同一构图。
- Visual evidence: mock-only `ui_demo` 冷启动、cache-refresh、error-recovery 和 Job Detail `job-detail-retry`，桌面 `1440x900` 与移动 `393x852`，再由主人确认。

## Visual Evidence

主人已确认以下 mock-only `ui_demo` 证据准确反映当前实现。页面级证据采用 `trim_only`，移动端使用 `393x852` CSS px；Job Detail 错误态使用桌面 `32px` 的插图-恢复组间距、移动端 `28px`，并使用桌面 `-6px`、移动端 `-5px` 的光学位移校正。

- 冷启动骨架（桌面）：`assets/queue-cold-desktop.png`
- 缓存刷新磨砂遮罩（桌面）：`assets/queue-cache-refresh-desktop.png`
- 错误遮罩与重试（桌面）：`assets/queue-error-desktop.png`
- 冷启动骨架（移动）：`assets/queue-cold-mobile.png`
- 缓存刷新磨砂遮罩（移动）：`assets/queue-cache-refresh-mobile.png`
- Job Detail 错误态（亮色桌面）：`assets/job-detail-retry-error-light-desktop.png`
- Job Detail 错误态（暗色桌面）：`assets/job-detail-retry-error-dark-desktop.png`
- Job Detail 错误态（亮色移动）：`assets/job-detail-retry-error-light-mobile.png`
- Job Detail 错误态（暗色移动）：`assets/job-detail-retry-error-dark-mobile.png`
- Job Detail 恢复态（亮色桌面）：`assets/job-detail-retry-ready-light-desktop.png`
- Job Detail 恢复态（暗色桌面）：`assets/job-detail-retry-ready-dark-desktop.png`
- Job Detail 恢复态（亮色移动）：`assets/job-detail-retry-ready-light-mobile.png`
- Job Detail 恢复态（暗色移动）：`assets/job-detail-retry-ready-dark-mobile.png`

## Related ADRs

- None

## 风险 / 假设

- 假设：现有 60 秒 fresh 决策继续有效，且 snapshot 仅承载既有只读 read model。
- 风险：状态域拆分不完整会继续让局部失败污染整页；所有 catch 分支必须明确进入 error 或真实 offline。
- 风险：查询提交如果混合 pending/committed 状态会造成标签与列表错配；分页和筛选需要同步提交。

## 参考

- `docs/specs/r8kpa-web-pwa-offline-shell/SPEC.md`
- `docs/specs/c2r2u-detail-route-service-navigation/SPEC.md`
- `docs/specs/e3f83-ghcr-webhook-inbox-sse/SPEC.md`
- `docs/specs/kdapc-version-inference-decouple/SPEC.md`
