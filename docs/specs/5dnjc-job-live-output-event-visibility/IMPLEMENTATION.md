# Implementation

## 状态

已实现，并补充成功 Compose pull 聚合摘要的 transient progress 清理，进入验证收口。

## 计划覆盖

- Rust：原始字节块 runner、VT100 0.16 parser、stdout/stderr 合并、管道裸 `LF` 的有状态 `CRLF` 行规整、50ms 快照节流、transient `commandSeq`、命令完成标记、SSE 多路复用和任务终态清理；订阅断开释放最后一个 hub 条目。Compose plugin pull 使用 `COMPOSE_PROGRESS=tty` 与 `COMPOSE_ANSI=always`，standalone `docker-compose` 另通过 Unix `script -e` 获得 PTY 并传播子命令退出码（运行时镜像安装 `util-linux`），且内部路由标记会在 spawn 前剥离。pull 进度解析让 stdout/stderr 以回调到达顺序共用跨块缓冲，失败分类同时保留两侧输出。成功 plugin/standalone Compose pull 的持久化结果只保留状态，失败输出仍保留，进度摘要解析清理 ANSI 控制序列后再计算，service-log partial buffer 限制为 64 KiB，并在强制分片时保持 UTF-8 边界和实时续行换行。
- Web：同一 `commandSeq` 的终端快照替换与冻结、空等级列、受限 ANSI 样式、同连接内摘要顺序去重、EVEN localStorage 偏好和日志工具栏开关；同长度快照更新时仍保持自动跟随，暂停跟随不回跳。
- Web：任务详情 `AsyncDataRegion` 建立桌面 flex 高度链和 `16px` 卡片间距；桌面日志视口独立滚动，`760px` 及以下解除日志局部高度与 overscroll 捕获，由主内容连续滚动。
- Storybook/ui_demo：终端回车进度替换、样式 segments、实时增长、去重、EVEN 开关、自动跟随与暂停跟随场景。
- 收口结构：service-log 解析回归测试移到独立模块，mock terminal 场景保持行为不变并满足仓库单文件预算（`install.ts` 保持在 1200 行上限内）。

## 验证

- Rust：`cargo fmt --all -- --check`、`cargo test --workspace`（695 API、1 common、58 supervisor）和 `cargo clippy --workspace --all-targets --all-features -- -D warnings` 已通过；PTY 路由覆盖 Compose V1 管道退化、原始控制序列透传、内部标记剥离与超时后子进程清理。
- Web：`bun run test`（138 tests）、`bun run lint`（0 errors，保留 2 条既有 warning）和 `bun run build` 已通过。
- Storybook：`build-storybook` 与 `test-storybook`（306 stories）已通过；`Pages/JobDetailPage/LiveOutputAndEventToggle` 的 play 覆盖终端快照、空等级列、不产生 WARN、EVEN 默认隐藏与开关显示、实时摘要去重。
- ui_demo：mock-only demo 的桌面与 `393x852` 截图验证了终端快照替换、ANSI 样式、空等级列、默认隐藏 EVEN 和窄屏布局；证据见 `SPEC.md` 的 `## Visual Evidence`。
- 本轮布局回归：`ui_demo` 在 `1280x720` 验证摘要/日志卡 `16px` 间距、日志视口 `scrollHeight > clientHeight` 且主内容不滚动；在 `393x852` 验证日志视口随内容展开、滚轮推进主内容且日志视口保持键盘可聚焦。Storybook 交互脚本同步覆盖同一合同。

## Remaining gaps

- 终端快照不提供断线补播；刷新或重连仍只恢复数据库摘要，这是既定非目标。
