# History

## 2026-08-03

从 legacy compose updater 计划中的任务事件、HTTP 和数据库契约收敛为 topic-level canonical spec。保留 legacy 源文件，待后续获得删除确认后再处理。

关键决策：原始 Docker/Compose 输出只通过无 id 的临时 SSE 实时展示，不新增逐行数据库日志或持久化 command id；EVEN 默认隐藏并由浏览器 localStorage 记忆。
