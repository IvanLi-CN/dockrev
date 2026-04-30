# Dockrev：自动部署策略配置器 演进历史（#xyy72）

> 这里记录会影响 Agent 理解“为什么一步步变成现在这样”的关键演进；单次任务流水账不放这里，规范正文仍以 `./SPEC.md` 为准。

## Decision Trace

- 2026-04-30: 创建 topic-level spec，锁定 Service/Stack 自动部署策略首版范围。
- 2026-04-30: 实现自动策略配置器、后端执行路径、Storybook 证据与共享测试机 smoke；远端 push/PR 截图提交仍等待主人授权。

## Key Reasons / Replacements

- 自动部署延迟采用“时间 + 推迟版本数”两个门槛叠加，原因是用户需要同时控制发布观察窗口与版本节奏，而不是二选一。
- `glob` 被纳入首版匹配器，原因是 Docker tag 通配符比 regex 更适合常见 tag 命名习惯，同时仍可安全转为 anchored regex 实现。
- 自动执行只绑定定时检查与 GHCR webhook 检查，原因是 UI 手动扫描常用于观察与诊断，不能隐式升级为部署行为。

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
