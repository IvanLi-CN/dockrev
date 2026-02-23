# Dockrev: 部署检查清单页（运维功能完整性判定）（#pgbb2）

## 状态

- Status: 已完成
- Created: 2026-02-23
- Last: 2026-02-23

## Goal

- 提供独立的部署欢迎/检查页面，面向运维快速确认“功能是否因配置缺失而不可用”。
- 检查模型仅输出 `pass/fail/na`，并给出唯一整体结论 `PASS/FAIL`，避免含糊状态。
- 保持最小配置部署友好：未启用的可选功能标记为 `NA`，不阻塞整体通过。

## In / Out

### In scope

- 新增 `GET /api/deploy-check/report`（只读、零副作用）并输出 `overall + checks`。
- 新增 `GET/PUT /api/deploy-welcome`，持久化“是否不再自动打开部署页”偏好。
- 首页首次访问自动跳转到部署检查页（受偏好控制）；设置页可手动再次打开。
- 部署检查页使用独立布局，不复用 Dashboard 的导航/信息布局。
- 检查清单按“核心功能 / 条件功能”分组，逐项展示 `summary/impact/recommendation/evidence`。

### Out of scope

- 本计划不接入“启动时硬失败并非 0 退出”主进程机制。
- 不做远端 registry 探测，不依赖最近 jobs/runtime scan 的时效性数据。
- 不改动更新策略本身（仅提供可用性判定与运维提示）。

## Acceptance Criteria

- Given 请求 `GET /api/deploy-check/report`
  When 未触发任何扫描
  Then 仍可返回完整报告，且不会新增 `jobs/job_logs` 数据。

- Given 至少一项 `required=true` 检查为 `fail`
  When 打开部署检查页
  Then 顶部整体结论显示 `FAIL`，并列出 `blockingCheckIds`。

- Given 可选功能未启用
  When 生成报告
  Then 该项状态为 `na` 且 `required=false`，不影响整体 `PASS` 判定。

- Given 用户勾选“不再自动显示此页面”并点击“进入 Dashboard”
  When 再次访问首页
  Then 不再自动跳转部署检查页；在设置页手动打开仍可访问该页。

- Given 访问 `/deploy-check`
  When 页面渲染
  Then 页面为独立视觉容器（无应用主 layout），且检查条目以 checklist 形式呈现。

## Testing

- `cargo test -p dockrev-api`
- `bun run --cwd web build`

## Risks

- `feature.registry_auth` 的 required 判定需严格本地规则，避免误判导致“最小部署”被错误阻塞。
- 页面独立布局若样式隔离不完整，可能被全局样式污染，导致视觉与交互偏离 checklist 设计。

## Milestones

- [x] M1: 完成后端只读能力判定接口与 deploy welcome 偏好持久化
- [x] M2: 完成独立部署检查页（自动跳转、设置页入口、checklist UI）
- [x] M3: 完成自动化验证并输出 PR 交付

## Change log

- 2026-02-23: 创建计划并冻结范围、验收、测试与风险（Status=待实现）。
- 2026-02-23: 完成实现与修复并创建 PR #87；CI 绿灯，计划收口为已完成。
