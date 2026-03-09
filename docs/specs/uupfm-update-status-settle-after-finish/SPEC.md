# Dockrev：更新完成后状态自动收敛

- ID: `uupfm`
- Status: `已验证`
- Owner: `backend + web`
- Last: `2026-03-08`
- Related: `q6x2g` `7ruev` `pv9vc`

## 背景

更新按钮当前会依据 update job 的 `queued/running` 状态正确显示 spinner，并在终态停止 spinner。

但 `可更新 / 需确认 / 架构不匹配 / 被阻止` 等状态来源于 stacks/services 快照；现状里 update job 终态不会保证这份快照立刻收敛，导致按钮已停止 loading，而页面行状态与汇总仍短时间停留在旧值。

## 目标

- update job 成功后，受影响 stack/service 的 persisted 状态在 job 结束前完成收敛。
- Overview、Services、Service Detail 在 update job 终态时自动刷新受影响目标，不需要手动刷新页面。
- 处理轻微时序延迟：首刷若命中旧值，补刷应自动收敛。

## 非目标

- 不改 update 按钮文案、跳转交互或 queue 页面结构。
- 不扩展到非 update 任务。
- 不引入整页硬刷新作为默认实现。

## 实现约束

- 后端优先复用现有 runtime digest / service check 持久化逻辑，避免新增第二套 candidate 判定规则。
- 前端仅增加 Web 内部终态事件，不修改 `/api/updates` HTTP 入参。
- 定向刷新优先按 stack patch `getStack()`；仅在无法定位目标 stack 时回退全量 `refresh()`。

## 验收标准

1. Given 单 service update 成功，When job 进入终态，Then 对应行在当前页自动清除已失效的 `可更新` 状态。
2. Given stack/all update 成功，When job 进入终态，Then 对应 stack 汇总与顶部统计自动收敛，无需手动刷新。
3. Given update job 终态先到、状态快照稍后收敛，When 页面收到终态事件，Then 立即刷新 + 短延迟补刷后能自动收敛。
4. Given update job 失败，When job 进入终态，Then spinner 停止，但页面不会错误清空 candidate。
5. Given 用户从列表跳到 job detail 再返回，When job 尚未终态，Then 既有 spinner 保持行为不回退；When 已终态，Then 返回后看到的是收敛后的状态。

## 里程碑

- [x] M1: 后端在 update 成功路径补齐 service 状态收敛。
- [x] M2: 前端 update tracker 在 job 终态发布内部事件。
- [x] M3: Overview / Services / Service Detail 接入定向刷新 + 补刷。
- [x] M4: 补齐后端测试与 Storybook 回归。

## 实现结果

- 后端在 update success 路径中同步收敛受影响 service 的 runtime/current/candidate 快照，并记录 `update_state_settled` job log。
- 前端 tracker 在 update job 终态发布 `dockrev:update-job-settled` 内部事件；Overview、Services、Service Detail 先定向刷新，再做一次短延迟补刷。
- Storybook mock 增加“job 先终态、stack 快照稍后收敛”的延迟场景，用于验证补刷链路。

## 验证

- `cargo test -p dockrev-api update_`
- `bun run --cwd web lint`
- `bun run --cwd web build`
- `bun run --cwd web build-storybook`
- `bun run --cwd web test-storybook`
