# Dockrev 异步数据连续性与加载反馈 实现状态

> 当前有效规范以 `./SPEC.md` 为准；这里仅记录实现覆盖和验证事实。

## Current Status

- Implementation: Job Detail 本地实现、视觉确认与交互验证完成；进入 PR 收口
- Lifecycle: active
- Delivery flow: fast-track；目标为一个直接 PR 的 `Step 5C Ready`。

## Coverage / rollout summary

- 已实现：`AsyncDataPhase`/`AsyncDataSource`、区域级骨架、延迟磨砂遮罩、独立首屏错误态、以 Adobe Spectrum `@spectrum-icons/illustrations` `Error` 的透明 SVG 表达读取故障、错误覆盖层与重试语义；资产随附 Apache-2.0 许可。组件通过 Dockrev 主题 token 复用同一份官方几何，以亮暗色板控制颜色，并对服务器线条的右侧视觉重量做小幅光学居中。减少动效环境保留状态文案。插图的视觉合同与审查记录见 [VISUAL_REVIEW.md](./VISUAL_REVIEW.md)。
- 已迁移：首页、服务大盘、Queue、版本推测、Stack、Service 只读数据域、Job Detail、系统设置、部署检查、GHCR 三页和服务树的冷启动、刷新、局部失败与查询竞态状态。
- 已实现：v2 fresh snapshot 的版本/readiness/committed-query-key 合同，以及资源历史按当前时间窗裁剪。
- 已验证：mock 路由延迟/失败合同、Storybook 状态矩阵、Web 单测、lint、production/demo 构建和全量 Storybook 交互巡检。
- 已收敛：服务详情核心请求使用独立错误覆盖层与重试；只在只读数据域完整 ready 后写入快照；部署检查、GHCR 刷新和 Job Detail 故事均丢弃旧请求或阻止重复触发。
- 已补强：服务详情设置/回滚/备份请求按数据域保留成功内容，用户触发刷新保持 200ms 门槛，缓存备份在 live 域失败时继续作为背景且相关写操作保持禁用。
- 已完成：二次刷新优先使用已提交 live 备份，Stack 与 GHCR 的刷新/分页控件在请求开始即禁用。
- 已实现：Job Detail 使用按 `jobId` 复用的快照协调器合并首屏读取、管理事件重同步和 SSE 对账；每次自动 GET 有 10 秒截止、最多一次 1 秒后重试，用户刷新可取消并替代自动序列，卸载与任务切换会清理请求、计时器和 SSE。
- 已覆盖：Job Detail 协调器单测验证单飞、超时、一次自动重试、手动替换、忽略 abort 的 loader 取消和 dispose；Storybook `InitialLoadRecovery` 与 mock-only `job-detail-retry` 验证两次失败后的错误态和手动恢复。
- 视觉证据：主人已确认既有队列状态证据，以及 Job Detail 错误/恢复态的亮暗主题桌面与 `393x852` 移动端证据；最终文件见 `SPEC.md` 的 `Visual Evidence`。

## References

- `./SPEC.md`
- `./HISTORY.md`
