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
  - In `web/`: `node ./scripts/storybook-build.mjs` and `DOCKREV_TEST_STORYBOOK_SMOKE_ONLY=1 bun run test-storybook` (421 stories passed, including both selected-version submission flows).
- Rollout facts: 新安装和升级数据库均从空的历史归属表开始，只积累今后成功检查产生的观察。

## Empirical Acceptance

- Scenario: 在隔离 Compose 服务上先以 `latest` 观察 D34，再观察 D37；版本预检将已观测 D37 分类为普通更新，将未观测 D38 分类为强制更新。普通指定版本任务完成后，将 Registry 的 `latest` 推进到 D38，并通过计划检查验证自动策略。
- Result: D34、D37、D38 摘要分别为 `sha256:b0d712eccfb116298225e4d51c03aee625d16a00d1ab9c9868dcdcfecfc4d`、`sha256:81cfed97581757e1d0ac95be6fd7e81845a7015a9d761f9739ed29ea1beb482d`、`sha256:18b2682426c42796bc896707d0cb947a6dfee03733c7c799e3ba3bba28f5738e`。D37 普通指定更新任务 `job_01M3PSEVH2C05DWDWETWZ21NYF` 成功；计划检查 `chk_01M3PSNJX2YFTNT4F3P6H4Y41B` 观测 D38 后，自动策略任务 `job_01M3PSNREE8Z3KKV17W698RW0E` 从 D37 成功推进至 D38。运行镜像、本地 `latest` 和 D38 摘要一致；Compose 文件 SHA-256 在更新前后均为 `f81eaa19847b54e8d23f123cd65454eca29c10165ec3317c32d329aeabc0a73c`。自动策略和计划检查设置已恢复。
- Evidence: 检查、预检、更新任务、最终服务状态、恢复后的策略设置和镜像/Compose 摘要保存在 `/srv/codex/agents/01a0d6f3-3124-7432-9daa-99fd4bfb3755/validation-08a2c8e7/evidence`。
- Test transport: 测试 Registry 使用任务自有 TLS CA。为让隔离应用访问该 CA，验证构建临时启用了 Reqwest native-root feature；仓库依赖配置和正式运行时 TLS 配置未改变。自动策略拉取可选兼容标签 `2.71.38` 时 Registry 未提供该别名，产生非阻塞告警，D38 摘要部署成功。
- Candidate binding: 最终候选 SHA、验收合同摘要、必需场景摘要和最终运行日志位置保存在当前交付流的 Candidate evidence card 中。

## Related Changes

- None

## References

- `./SPEC.md`
- `./HISTORY.md`
