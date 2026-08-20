# Dockrev 异步数据连续性与加载反馈 实现状态

> 当前有效规范以 `./SPEC.md` 为准；这里仅记录实现覆盖和验证事实。

## Current Status

- Implementation: 本地实现与 mock-only 视觉确认完成；进入 PR 收口
- Lifecycle: active
- Delivery flow: fast-track；目标为一个直接 PR 的 `Step 5C Ready`。

## Coverage / rollout summary

- 已实现：`AsyncDataPhase`/`AsyncDataSource`、区域级骨架、延迟磨砂遮罩、错误覆盖层与重试语义；减少动效环境保留状态文案。
- 已迁移：首页、服务大盘、Queue、版本推测、Stack、Service 只读数据域、Job Detail、系统设置、部署检查、GHCR 三页和服务树的冷启动、刷新、局部失败与查询竞态状态。
- 已实现：v2 fresh snapshot 的版本/readiness/committed-query-key 合同，以及资源历史按当前时间窗裁剪。
- 已验证：mock 路由延迟/失败合同、Storybook 状态矩阵、Web 单测、lint、production/demo 构建和全量 Storybook 交互巡检。
- 已收敛：服务详情核心请求使用独立错误覆盖层与重试；只在只读数据域完整 ready 后写入快照；部署检查、GHCR 刷新和 Job Detail 故事均丢弃旧请求或阻止重复触发。
- 已补强：服务详情设置/回滚/备份请求按数据域保留成功内容，用户触发刷新保持 200ms 门槛，缓存备份在 live 域失败时继续作为背景且相关写操作保持禁用。
- 视觉证据：主人已确认 `assets/queue-cold-desktop.png`、`assets/queue-cache-refresh-desktop.png`、`assets/queue-error-desktop.png`、`assets/queue-cold-mobile.png`、`assets/queue-cache-refresh-mobile.png`；覆盖桌面与 `393x852` 移动端。

## References

- `./SPEC.md`
- `./HISTORY.md`
