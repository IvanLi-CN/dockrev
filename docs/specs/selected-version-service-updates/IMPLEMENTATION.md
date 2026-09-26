# 服务版本列表指定版本更新实现状态

> `./SPEC.md` 是稳定需求合同；本文件记录当前实现覆盖和验证事实。

## Current Status

- Implementation: 功能与自动验证完成；视觉证据已获主人确认并归档；共享测试机 Docker/Compose 经验验收通过；Tier 3 PR 收敛待完成
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
- Result: `v2.71.34` 检查后，历史关联使 `v2.71.37` 走普通指定部署并成功；未观测的 `v2.71.38` 预检分类为强制。随后真实计划检查和自动策略任务成功将服务推进到 `v2.71.38`。各步运行摘要与本地 `latest` 摘要一致，Compose 文件 SHA-256 保持不变。
- Evidence: `/srv/codex/agents/01a0d6f3-3124-7432-9daa-99fd4bfb3755/dockrev-selected-version-final-c8ea2fb8-20260925T124015Z.log`（`empirical_acceptance=passed candidate_sha=c8ea2fb8a91d13b85db338b0122e603efec265f4`）；关联应用日志为同目录下对应的 `-app.log`。测试实例的 Docker 项目发现过滤器只暴露该次隔离 Compose 项目；本次任务容器、网络和镜像引用均按精确身份清理。
- Test transport: loopback HTTP Registry 的验证二进制在测试机工作副本中临时使用 HTTP；仓库源码和最终构建仍使用 HTTPS。
- Candidate binding: 最终候选 SHA、验收合同摘要、必需场景摘要和最终运行日志位置保存在当前交付流的 Candidate evidence card 中。

## Remaining Gaps

- 完成 Tier 3 四通道审查、PR CI 和 Step 5C Ready 收敛；不合并。

## Related Changes

- None

## References

- `./SPEC.md`
- `./HISTORY.md`
