# 服务版本列表指定版本更新实现状态

> `./SPEC.md` 是稳定需求合同；本文件记录当前实现覆盖和验证事实。

## Current Status

- Implementation: 功能、自动验证、受控 Docker/Compose 行为验收和桌面/移动端视觉检查已完成
- Lifecycle: active
- Catalog note: 服务版本列表指定版本部署

## Implementation Coverage

- Requirement coverage: `REQ-SVSU-001`–`REQ-SVSU-007` 由当前 PR 实现。
- Persistence: 成功检查只记录服务、镜像仓库、当时配置标签、返回摘要和观测时间；迁移不回填；摘要先于或晚于版本推断都能按相同仓库和摘要绑定。
- API: 服务级观察查询、预检和提交接口已实现；普通更新使用历史摘要，强制更新实时解析原始 release tag，并在提交时重做预检。
- Execution: 选定摘要部署复用更新任务与回滚保护，跳过再次拉取配置标签，并将运行镜像同步到本地配置标签；Compose 文件不变。
- Automatic updates: 成功的计划或匹配 GHCR Webhook 检查可重验已存在候选，但只有观测服务、镜像仓库、当前配置标签和候选摘要全部一致，且候选摘要仍不同于当前部署时才重新进入自动策略。严格 SemVer 仅从该条摘要绑定观测传入结算；普通展示字符串仍不能跳过候选推断。策略变化等原因取消尚未启动的 auto-policy 更新时，同一事务会释放其服务接受状态代次；启动迁移也会恢复仍匹配已取消任务租约的旧代次，避免取消任务永久阻塞后续操作。
- UI: 普通/强制更新按钮和一层/两层确认已接入版本列表，其他更新入口沿用原行为。
- Verification commands:
- `cargo fmt --all -- --check`
- `cargo check -p dockrev-api --tests`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --release -p dockrev-api --bin dockrev -- --test-threads=1`（981 passed、1 ignored）
- Automatic-policy recheck tests: 11 passed; digest-bound settlement regression test: 1 passed; cancelled auto-policy service-lease release and startup recovery tests: 2 passed.
  - In `web/`: `bun run test`（249 passed、778 expectations）、`bun run lint`（0 errors；3 existing hook warnings）、`bun run build:demo:pages`
  - TypeScript build, selected-version Spec contract check, and visual-evidence document check passed; the related legacy service-detail Spec retains its historical format and does not pass the current canonical-format checker.
  - In `web/`: `node ./scripts/storybook-build.mjs` and `DOCKREV_TEST_STORYBOOK_INTERACTIVE_ONLY=1 node ./scripts/test-storybook.mjs`
- Rollout facts: 新安装和升级数据库均从空的历史归属表开始，只积累今后成功检查产生的观察。

## Empirical Acceptance

- Scenario: 在隔离 Compose 服务上先观察配置标签 `latest` 的 D34，再将 Registry 的 `latest` 指向 D38；从版本列表通过普通路径部署已观测 D37。随后让真实 cron 检查首次观察一个新的 D38 测试摘要，并由已启用的自动策略部署。
- Result: `v2.71.34` 检查后，历史关联使 `v2.71.37` 走普通指定部署并成功；未观测的 `v2.71.38` 预检分类为强制。最终计划检查 `chk_01M3J2FY74JM6VTCWC31DHPR6E` 成功观察到当前 `latest` 摘要 `sha256:bdc29646be584a68a1b4559aae6ba265f651d5ce11b4d15fcbc49d0676831099`，自动策略任务 `job_01M3J2G95KZE8J5NMFAKDQYMNY` 成功从 D37 推进到该 D38 摘要；运行镜像与本地 `latest` 摘要一致，Compose 文件 SHA-256 保持为 `9ffa0dbd18105e861ded817cf1ae7de3166da710c7607bbd626dcfae38a0d32f`。测试 Registry 为避免复用旧候选，以仅增加测试标签的镜像生成新摘要；自动策略拉取可选兼容标签 `2.71.38` 时 Registry 未提供该别名，产生非阻塞告警，目标摘要部署成功。
- Evidence: 当前候选 API 全量测试和上述检查/任务 JSON、镜像摘要、Compose SHA-256 及设置恢复结果保存在 `/srv/codex/agents/01a0d6f3-3124-7432-9daa-99fd4bfb3755/selected-version-fix-62bc150e-20260928/evidence-final`。
- Test transport: 测试机隔离工作副本使用临时 CA 信任配置访问带 TLS 的本地 Registry；该构建参数和证书未改仓库源码或正式构建配置。
- Candidate binding: 最终候选 SHA、验收合同摘要、必需场景摘要和最终运行日志位置保存在当前交付流的 Candidate evidence card 中。

## Remaining Gaps

- 完成 Tier 3 四通道审查、最终候选 CI 和 Step 5C Ready 收敛；不合并。

## Related Changes

- None

## References

- `./SPEC.md`
- `./HISTORY.md`
