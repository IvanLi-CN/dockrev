# Dockrev：Supervisor 自我升级页低拥挤度重构（#np5fm）

## 状态

- Status: 已完成
- Created: 2026-03-10
- Last: 2026-03-10
- Notes: fast-track · PR #160

## 背景 / 问题陈述

- 当前 `/supervisor/` 页面把标题、操作按钮、长状态文本、operation tabs 与日志区挤在两块紧凑卡片里，首屏扫描压力大，长字符串会进一步压缩按钮与留白。
- history tabs 依赖密集 pill 排列与颜色点，桌面端和移动端都容易出现“状态很多但难扫读”的问题。
- 该页面仍需保留 supervisor 独立控制台的即时监控语义：状态必须默认直出，不能为了减拥挤而把关键 target/previous/opId/step 收进二级折叠。

## 目标 / 非目标

### Goals

- 将 Supervisor 页面重构为四个稳定区块：`masthead`、`action deck`、`live status grid`、`history/log workspace`。
- 保留现有 dark/light theme、轮询、rollback 确认、running spinner、同源主题同步与所有接口 contract。
- 将当前 operation 状态改为更易扫读的键值块/代码块组合，保证长 target/previous 文本可换行且不撑爆布局。
- 将 history 从拥挤的 pill tabs 改为带状态文案的选择列表，保留“查看旧 operation 时不被新 operation 抢焦点”的行为。
- 提升桌面端日志可视高度，并在移动端提供无横向滚动的纵向堆叠布局。

### Non-goals

- 不修改 `/supervisor/self-upgrade`、`/supervisor/version`、`/supervisor/self-upgrade/rollback` 的 schema。
- 不修改升级执行逻辑、轮询间隔、rollback 判定规则或主题存储 key。
- 不将 Supervisor 页面迁移到 React / `web/` 主前端工程。

## 范围（Scope）

### In scope

- `crates/dockrev-supervisor/src/app/ui.rs`
- `crates/dockrev-supervisor/src/app/tests.rs`
- `docs/specs/README.md`

### Out of scope

- `web/src/**`
- supervisor Rust 路由 / 状态持久化 / 编排逻辑
- deploy、gateway、鉴权配置

## 设计决策（Implementation Notes）

- 延续 `ui-ux-pro-max` 的 monitoring/dashboard 方向：强调清晰层级、状态文本与宽松留白，而非堆叠更多 pill 控件。
- 页面容器提升到约 `1160-1240px`，使用 `24/20/16px` 主节奏，并通过桌面双栏 / 移动端单栏实现自适应。
- `live status grid` 至少拆出 `state`、`opId`、`step`、`startedAt/updatedAt`、`target`、`previous` 六类信息块；其中长镜像引用使用 `code/pre` 风格承载。
- `history/log workspace` 在桌面端为左侧 history 列表、右侧日志控制台；移动端切为 history 在上、日志在下。
- history 列表项必须同时提供颜色点与文字状态标签，避免只靠颜色表达运行态。

## 验收标准（Acceptance Criteria）

- Given 打开 `/supervisor/` 首屏，When 页面加载完成，Then 可见四个分层明确的区块，视觉上不再是两块高密度卡片连在一起。
- Given `state/opId/step/target/previous` 都存在，When 状态区渲染，Then 信息默认直出，长文本可以自然换行，不造成整体横向滚动。
- Given operation 数量较多，When history 渲染，Then 每项都包含时间、短 opId、状态文案与状态点；当前选中项清晰可辨。
- Given 用户正在查看历史 operation，When 轮询拿到新的最新 operation，Then history 只提示新项，不自动把日志视图切回最新项。
- Given 页面在 `375px / 768px / 1440px` 宽度下打开，When 检查布局，Then 不出现整体横向滚动，按钮区不互相挤压，桌面端日志区高度明显优于旧版。
- Given `running` / `rollback` / `theme sync` 场景，When 本次改动后回归，Then 既有行为保持兼容。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo test -p dockrev-supervisor`
- 本地浏览器 smoke：`http://127.0.0.1:50884/supervisor/`
  - 断点：375 / 768 / 1440
  - 主题：light / dark
  - 场景：idle + operation history + running buttons + rollback popconfirm

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 重构 `/supervisor/` HTML 骨架为四区布局，拉开容器宽度、卡片层级与响应式节奏。
- [x] M2: 将 status 从单行字符串升级为键值信息块，将 history tabs 重构为文本状态列表，并保留既有 selection / newer-item 语义。
- [x] M3: 补齐 render_ui 回归测试，覆盖新结构、running 状态、rollback 确认与主题 contract。
- [x] M4: 完成本地 `cargo test` + 浏览器多断点 smoke 验证。
- [x] M5: 快车道交付：提交、push、PR、checks、review-loop 收敛，并把结果回写 spec。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：内嵌 HTML/CSS/JS 一次性改动较大，若测试只断言字符串片段，容易遗漏运行态行为回归。
- 风险：history 列表重构后，若处理不好 latest/newer 标记逻辑，会破坏“查看旧 operation 时不跳转”的原语义。
- 假设：Supervisor 继续以静态页面实现即可满足本次改造，不需要组件化拆分到前端工程。
- 假设：默认全量直出状态信息仍是运维可读性的最佳权衡，只需要通过分区与换行来减压。

## 变更记录（Change log）

- 2026-03-10: 创建规格，冻结四区布局、默认状态直出、history 列表化与响应式验收口径。
- 2026-03-10: 完成 Supervisor 四区重构、history 列表化、render_ui 回归测试，以及本地 `cargo test` + Chrome DevTools 多断点主题 smoke。
- 2026-03-10: 根据 review-loop 收敛离线缓存态与无 history fallback，补充 uncached offline / logs-only 回归断言，并完成 PR #160、GitHub checks 全绿与 spec 回写。
- 2026-03-10: 修复查看旧 operation 时，同一最新 op 持续追加日志不会点亮“新日志”提示的问题，并补充对应前端状态机回归断言。
- 2026-03-10: 按一档比例下调整体视觉尺度，收紧容器宽度、卡片 padding、标题字号、状态块高度与历史/日志最小高度，消除页面“像被放大”的观感。
- 2026-03-10: 基于真实 DOM 复测继续压缩 masthead/action/status/workspace 的实际块高，将桌面端 masthead/action/status 从约 278/209/509px 收紧到约 220/181/410px，确保缩小效果肉眼可感。
- 2026-03-10: 用户对“房间更松”方案仍不满意后，改为重新实现视觉骨架：切换为 command-center 风格的双区 masthead、非对称 status board 与更克制的 history/log 控制台，同时保留所有既有 DOM id、状态机与接口 contract。
- 2026-03-11: 按“日志优先”要求再次重做布局，把 workspace 提前到 status 之前，并在桌面端改为按钮首屏可见 + 日志正文首屏可见的结构；Chrome DevTools 实测 1440 宽度下 `apply` 与 `#logs` 同时位于首屏内。
- 2026-03-11: 根据日志控制台视觉反馈，下调 console 标题、摘要与正文的字号/行高，避免首屏日志区在宽屏下显得过于膨胀。
- 2026-03-11: 将 Supervisor 版本 / 仓库 / 开发者信息从 masthead 挪到页脚信息条，避免占用首屏空间。
- 2026-03-11: 继续修正布局审美问题，将 workspace 重构为“日志主舞台 + 右侧状态/历史侧栏”，并把 status panel 收敛成下方 detail inspector，避免多个大卡片生硬纵向堆叠。
- 2026-03-11: 重做页脚元信息布局，移除胶囊式块状设计，改为带顶部细分隔线的低强调信息条。
- 2026-03-11: 基于真实 DOM 对齐复盘，重构 action deck 为共享网格（target / primary / aux / hints 两行对齐），并将 history hint 收回 sidebar 内部，消除页面右漂说明与三块孤岛式控件。
- 2026-03-11: 继续修复 workspace 空洞，将 `Operation detail` 并入日志主列下方，改成 `logs + detail` 左列与 `status + history` 右列的双层网格，避免桌面端左下区域大面积留白。
- 2026-03-11: 为日志控制台加入克制型语义高亮：按行渲染 `timestamp / level / message`，并对 `INFO/WARN/ERROR`、镜像引用、digest 与 opId 做前端 token 高亮，保持既有 schema 与轮询逻辑不变。
- 2026-03-11: 在语义高亮基础上继续为 `WARN/ERROR/FATAL` 日志行增加低饱和整行底色，强化异常扫描效率，同时保持 `INFO` 行背景安静不抢眼。
