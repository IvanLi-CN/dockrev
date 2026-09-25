# 服务版本列表指定版本更新实现状态

> `./SPEC.md` 是稳定需求合同；本文件记录当前实现覆盖和验证事实。

## Current Status

- Implementation: 功能与自动验证完成；视觉证据待主人确认后持久化，之后进入 Tier 3 PR 收敛
- Lifecycle: active
- Catalog note: 服务版本列表指定版本部署

## Implementation Coverage

- Requirement coverage: `REQ-SVSU-001`–`REQ-SVSU-007` 由当前 PR 实现。
- Persistence: 成功检查只记录服务、镜像仓库、当时配置标签、返回摘要和观测时间；迁移不回填；摘要先于或晚于版本推断都能按相同仓库和摘要绑定。
- API: 服务级观察查询、预检和提交接口已实现；普通更新使用历史摘要，强制更新实时解析原始 release tag，并在提交时重做预检。
- Execution: 选定摘要部署复用更新任务与回滚保护，跳过再次拉取配置标签，并将运行镜像同步到本地配置标签；Compose 文件不变。
- UI: 普通/强制更新按钮和一层/两层确认已接入版本列表，其他更新入口沿用原行为。
- Verification commands:
  - `cargo fmt --all -- --check`
  - `cargo test -p dockrev-api --bin dockrev -- --test-threads=1`（共享测试机，948 passed、0 failed、1 ignored）
  - In `web/`: `bun run test`（247 tests across 53 files）、`bun run lint`（0 errors；3 existing hook warnings）、`bun run build:demo:pages`
  - In `web/`: `node ./scripts/storybook-build.mjs` and `DOCKREV_TEST_STORYBOOK_INTERACTIVE_ONLY=1 node ./scripts/test-storybook.mjs`
  - Topic Spec contract and drift checks passed for this Spec and the associated service-detail subpage Spec.
- Rollout facts: 新安装和升级数据库均从空的历史归属表开始，只积累今后成功检查产生的观察。

## Empirical Acceptance

- Scenario: 在隔离 Compose 服务上先观察 `latest` 的 D34，再将 `latest` 指向 D37；检查后通过普通路径部署 D37，再让真实 cron 检查在策略启用时把服务推进到 D38。
- Result: 指定部署与自动策略任务均成功；每次运行摘要都等于本地 `latest` 摘要，Compose 文件哈希保持不变。
- Evidence: `/srv/codex/agents/01a0d6f3-3124-7432-9daa-99fd4bfb3755/dockrev-selected-version-e2e-5718409a.log`（`empirical_acceptance=passed selected_D37_then_scheduled_auto_D38`）。
- Test transport: 为隔离测试使用 loopback HTTP Registry 时，测试二进制临时启用了本地 HTTP scheme；最终源码的 Registry scheme 已恢复为 HTTPS。
- Note: 自动更新任务有一次既有兼容 tag `2.71.38` 拉取告警，但目标 `latest` 摘要拉取、部署和本地 tag 同步成功。

## Remaining Gaps

- 桌面与移动端截图已捕获，等待主人确认截图内容及允许将相同图片保存到 Spec/PR。
- 完成 Tier 3 四通道审查、PR CI 和 Step 5C Ready 收敛；不合并。

## Related Changes

- None

## References

- `./SPEC.md`
- `./HISTORY.md`
