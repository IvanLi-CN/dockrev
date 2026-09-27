# 服务版本列表指定版本更新实现状态

> `./SPEC.md` 是稳定需求合同；本文件记录当前实现覆盖和验证事实。

## Current Status

- Implementation: 功能、自动验证和受控 Docker/Compose 行为验收已完成；视觉证据已归档
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
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `cargo test -p dockrev-api --bin dockrev`（948 passed、1 transient failure、1 ignored；失败用例立即单测重试通过）
  - In `web/`: `bun run test`（249 passed、778 expectations）、`bun run lint`（0 errors；3 existing hook warnings）、`bun run build:demo:pages`
  - TypeScript build, selected-version Spec contract check, and visual-evidence document check passed; the related legacy service-detail Spec retains its historical format and does not pass the current canonical-format checker.
  - In `web/`: `node ./scripts/storybook-build.mjs` and `DOCKREV_TEST_STORYBOOK_INTERACTIVE_ONLY=1 node ./scripts/test-storybook.mjs`
- Rollout facts: 新安装和升级数据库均从空的历史归属表开始，只积累今后成功检查产生的观察。

## Empirical Acceptance

- Scenario: 在隔离 Compose 服务上先观察 `latest` 的 D34，再将 `latest` 指向 D37；检查后通过普通路径部署 D37，再让真实 cron 检查在策略启用时把服务推进到 D38。
- Result: `v2.71.34` 检查后，历史关联使 `v2.71.37` 走普通指定部署并成功；未观测的 `v2.71.38` 预检分类为强制。随后真实计划检查和自动策略任务成功将服务推进到 `v2.71.38`。各步运行摘要与本地 `latest` 摘要一致，Compose 文件 SHA-256 保持不变。自动策略尝试拉取可选兼容标签 `2.71.38` 时测试 Registry 未提供该别名，因此有非阻塞告警；目标摘要部署成功。
- Evidence: `/srv/codex/agents/01a0d6f3-3124-7432-9daa-99fd4bfb3755/selected-version-final-19042071-20260927t051612z/evidence.log`（实现候选 `190420710dd736822da1a16c40efb6f50b6da840`）；Compose 文件 SHA-256 前后均为 `456560989e96fdaab1958f14228a61e8fd04cf91e9efd063814a95ab925d5b8f`。关联应用日志位于同一目录的 `app.log`。
- Test transport: 测试机隔离工作副本使用临时 CA 信任配置访问带 TLS 的本地 Registry；该构建参数和证书未改仓库源码或正式构建配置。
- Candidate binding: 最终候选 SHA、验收合同摘要、必需场景摘要和最终运行日志位置保存在当前交付流的 Candidate evidence card 中。

## Remaining Gaps

- 完成 Tier 3 四通道审查、PR CI 和 Step 5C Ready 收敛；不合并。

## Related Changes

- None

## References

- `./SPEC.md`
- `./HISTORY.md`
