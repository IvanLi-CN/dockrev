# History

## 2026-08-03

从 legacy compose updater 计划中的任务事件、HTTP 和数据库契约收敛为 topic-level canonical spec。保留 legacy 源文件，待后续获得删除确认后再处理。

关键决策：原始 Docker/Compose 输出只通过无 id 的临时 SSE 实时展示，不新增逐行数据库日志或持久化 command id；EVEN 默认隐藏并由浏览器 localStorage 记忆。

## 2026-08-04

线上任务详情显示 Docker layer `Downloading` 进度重复堆叠，且实时 stderr 被前端映射为 WARN。根因是原始控制序列被按字符串行切分，`\r` 更新被误当作新行。

改为 `vt100` 0.16 后端终端模拟：runner 传递原始 `Vec<u8>` 块，stdout/stderr 合并解析，`job_live_terminal` 只发送无 id、无持久化的完整可见行快照；使用 240x200 屏幕、2000 行滚屏和 50ms 节流。每个命令使用 transient `commandSeq`，前端按序替换快照、命令完成后冻结，并让实时行等级列留空。旧的持久化摘要、REST 结构、Last-Event-ID 和断线恢复不变。

收口审查补充：纯 VT100 控制序列或清屏导致最终快照为空时，不抑制后续持久化命令摘要；订阅断开、跨块回车进度和无换行 service-log 缓冲均保持有界且可清理。

进一步收口：累计此前已发送的可见终端快照，避免清屏后的命令摘要重复；service-log 强制分片保留 UTF-8 code point 边界，并为无时间戳实时续行恢复换行分隔。

修正管道输出的终端行规整：VT100 的裸 `LF` 只下移光标而不回到第 0 列，导致 Docker layer 进度呈阶梯状右移。新增跨 chunk 的有状态规整器，仅将裸 `LF` 转为 `CRLF`，不重复转换已有 `CRLF` 或改变独立 `CR`，并在进入 parser 前统一应用。

门禁收口：将 service-log 解析回归测试拆到独立测试模块，压缩 mock terminal 场景的重复排版；运行时契约与测试覆盖保持不变。
