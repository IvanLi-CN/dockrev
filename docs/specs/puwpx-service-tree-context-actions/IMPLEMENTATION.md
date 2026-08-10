# Dockrev：服务树上下文快捷操作 实现状态（#puwpx）

## Current Status

- Implementation: 已完成
- Lifecycle: active
- Catalog note: fast-track（服务树上下文菜单 + Stack lifecycle queue）

## Coverage / rollout summary

- `stack_lifecycle` 已接入 HTTP、Compose runner、队列审计、任务目标持久化与双向冲突锁。
- 服务树已接入 shadcn/Radix ContextMenu、实时状态、直接提交、禁用原因、Supervisor 例外和 Toast 任务入口。
- Stack 详情页已复用服务详情生命周期控件：桌面 split action、移动 Stack 操作菜单、状态轮询、活动 Job 入口与结算刷新均已接入。
- Stack 详情与 Stack 右键的启动直接提交，停止/重启使用 Stack 名称和受影响服务数确认；Service 右键继续直接提交。
- Compose capability 统一探测 V2 plugin/standalone 版本；生命周期写操作和后台任务在执行前拒绝 V1/失败版本，返回 `compose_v2_required`。
- Stack/服务运行态读取先校验配置服务定义，合法配置下空 `ps -a` 返回 `stopped`，查询失败保持 `unknown`。
- mock API 已覆盖 Stack running/stopped/partial/unknown/active 场景；Stack 页面与服务树 Storybook 已覆盖动作发现、确认取消/提交和移动菜单。
- Web 构建、demo 构建、Storybook 构建与 321 个交互故事测试通过；lint 仅保留既有 TanStack Virtual React Compiler 警告。
- 最终验证覆盖 Compose V2 plugin/standalone、V1 拒绝、空容器 `stopped` 判定和生命周期写入口门禁。

## Remaining Gaps

- mock-only Storybook canvas 桌面与 `393x852` 移动视觉证据已落盘到 `assets/stack-lifecycle-desktop.png` 与 `assets/stack-lifecycle-mobile.png`，并生成不可变聊天快照；Impeccable detector 与 spec drift check 已通过。
- 本地实现已达到功能验证门槛；生产 101 仅执行只读复现，未执行远端服务变更。

## References

- `./SPEC.md`
- `./HISTORY.md`
- `./contracts/http-api.md`
