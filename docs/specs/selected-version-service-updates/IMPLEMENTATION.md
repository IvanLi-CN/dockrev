# 服务版本列表指定版本更新实现状态

> `./SPEC.md` 是稳定需求合同；本文件记录当前实现覆盖和验证事实。

## Current Status

- Implementation: 功能、当前候选自动验证、受控 Docker/Compose 行为验收和桌面/移动端视觉检查已完成
- Lifecycle: active
- Catalog note: 服务版本列表指定版本部署

## Implementation Coverage

- Requirement coverage: `REQ-SVSU-001`–`REQ-SVSU-007` 由当前 PR 实现。
- Persistence: 成功检查只记录服务、镜像仓库、当时配置标签、返回摘要和观测时间；迁移不回填；摘要先于或晚于版本推断都能按相同仓库和摘要绑定。
- API: 服务级观察查询、预检和提交接口已实现；普通更新使用历史摘要，强制更新实时解析原始 release tag，并在提交时重做预检。指定版本入队事务还会原子复核服务及其 Stack 未归档；覆盖 Registry 查询期间服务或 Stack 被归档的接口回归测试已通过。
- Execution: 选定摘要部署复用更新任务与回滚保护，跳过再次拉取配置标签，并将运行镜像同步到本地配置标签；Compose 文件不变。
- Automatic updates: 成功的计划或匹配 GHCR Webhook 检查可重验已存在候选，但只有观测服务、镜像仓库、当前配置标签和候选摘要全部一致，且候选摘要仍不同于当前部署时才重新进入自动策略。严格 SemVer 仅从该条摘要绑定观测传入结算；普通展示字符串仍不能跳过候选推断。策略变化等原因取消尚未启动的 auto-policy 更新时，同一事务会释放其服务接受状态代次；启动迁移也会恢复仍匹配已取消任务租约的旧代次，避免取消任务永久阻塞后续操作。
- UI: 普通/强制更新按钮和一层/两层确认已接入版本列表，其他更新入口沿用原行为。
- Verification commands:
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`（当前候选通过）
  - `cargo test --workspace --locked --all-features -- --test-threads=2`（当前候选 984 passed、1 ignored；另一个 crate 58 passed）
  - Automatic-policy recheck tests: 11 passed; digest-bound settlement regression test: 1 passed; cancelled auto-policy service-lease release, startup recovery, and stale update-guard enqueue tests passed.
  - In `web/`: `bun run build`（264 tests passed、830 assertions）、`bun run lint`（0 errors；3 existing hook warnings）、`bun run build:demo:pages`
  - `python scripts/check-pwa-assets.py`（通过）；Storybook production build 和 smoke（421 stories passed）
  - TypeScript build、selected-version Spec contract check、visual-evidence document check 和 `git diff --check` 均通过；相关旧版 service-detail Spec 保留历史格式，不通过当前 canonical-format checker。
- Rollout facts: 新安装和升级数据库均从空的历史归属表开始，只积累今后成功检查产生的观察。

## Empirical Acceptance

- Scenario: 在隔离 Compose 服务上以 D34 为当前摘要，将 Registry 的 `latest` 指向 D37 并完成成功检查；D37 预检分类为普通更新，指定版本任务按其观测摘要部署。随后将 `latest` 指向新的 D38 测试摘要，通过真实的计划检查和自动策略任务推进到 D38。
- Result: D34、D37、D38 摘要分别为 `sha256:b0d712eccfb116298225e4d51c03aee625d16a00d1ab9c9868dcdcfecfc4d`、`sha256:81cfed97581757e1d0ac95be6fd7e81845a7015a9d761f9739ed29ea1beb482d`、`sha256:691df418ed6f4c1986bdc4afdc52904d9f896a0b7b83fe9dc68dc5f60ce8c8b1`。D37 预检分类为普通更新，任务 `job_01M3RJP8EJE8TV53NXZQXNEGAZ` 成功从 D34 更新至 D37，`skipTargetTagPull=true` 且 `pullTags=[]`；过期预检提交被拒绝，刷新预检后成功入队。计划检查 `chk_01M3RJTT221YAZYFM45QE7J42B` 成功观察 D38，自动策略任务 `job_01M3RJTV6KDXXH0XRK3BY626CJ` 成功将服务从 D37 推进至 D38。最终运行镜像和本地 `latest` 均指向 D38；Compose 文件 SHA-256 在 D37 部署后和 D38 自动更新后均为 `bb3de43c96dab21d04463fa80312312bb54a486c5540a87aba9f667289c337f0`。
- Evidence: 当前候选 `e0eb2b2f37ddff3b486ad1bfff949a8f4a83686d` 的检查、预检、指定更新、计划检查、自动策略任务、最终运行摘要及 Compose 文件摘要保存在 `/srv/codex/agents/01a0d6f3-3124-7432-9daa-99fd4bfb3755/validation-archive-guard/a4/evidence`。计划和服务自动更新设置已恢复；测试 API、Compose 项目和 Registry 已停止，证据保留在 Agent Directory。
- Test note: 自动策略成功部署 D38；隔离 Registry 未提供派生兼容标签 `2.71.38`，因此兼容标签拉取产生非致命告警，基于配置标签 `latest` 的摘要拉取和服务更新仍成功。测试 Registry 使用任务自有 TLS CA；验证构建临时启用了 Reqwest native-root feature，仓库依赖配置和正式运行时 TLS 配置未改变。D38 是隔离 Registry 内的测试镜像，不代表公开 Registry 镜像。
- Candidate binding: 最终候选 SHA、验收合同摘要、必需场景摘要和最终运行日志位置保存在当前交付流的 Candidate evidence card 中。

## Related Changes

- None

## References

- `./SPEC.md`
- `./HISTORY.md`
