# 服务版本列表指定版本更新实现状态

> `./SPEC.md` 是稳定需求合同；本文件记录当前实现覆盖和验证事实。

## Current Status

- Implementation: 功能、当前候选自动验证、受控 Docker/Compose 行为验收和桌面/移动端视觉检查已完成
- Lifecycle: active
- Catalog note: 服务版本列表指定版本部署

## Implementation Coverage

- Requirement coverage: `REQ-SVSU-001`–`REQ-SVSU-007` 由当前 PR 实现。
- Persistence: 成功检查只记录服务、镜像仓库、当时配置标签、返回摘要和观测时间；迁移不回填；摘要先于或晚于版本推断都能按相同仓库和摘要绑定。
- API: 服务级观察查询、预检和提交接口已实现；普通更新使用历史摘要，强制更新实时解析原始 release tag，并在提交时重做预检。指定版本入队事务还会原子复核服务及其 Stack 未归档；接口回归测试覆盖 Registry 查询期间归档冲突、强制确认缺失拒绝，以及强制确认后解析摘要并成功入队。
- Execution: 选定摘要部署复用更新任务与回滚保护，跳过再次拉取配置标签，并将运行镜像同步到本地配置标签；Compose 文件不变。
- Automatic updates: 成功的计划或匹配 GHCR Webhook 检查可重验已存在候选，但只有观测服务、镜像仓库、当前配置标签和候选摘要全部一致，且候选摘要仍不同于当前部署时才重新进入自动策略。严格 SemVer 仅从该条摘要绑定观测传入结算；普通展示字符串仍不能跳过候选推断。策略变化等原因取消尚未启动的 auto-policy 更新时，同一事务会释放其服务接受状态代次；启动迁移也会恢复仍匹配已取消任务租约的旧代次，避免取消任务永久阻塞后续操作。
- UI: 普通/强制更新按钮和一层/两层确认已接入版本列表，其他更新入口沿用原行为。
- Verification commands:
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`（当前候选通过）
  - `cargo test --workspace --locked --all-features -- --test-threads=2`（主 crate 988 passed、1 ignored；另一个 crate 58 passed）
  - `cargo test -p dockrev-api --locked --all-features selected_version -- --test-threads=2`（11 passed；覆盖指定版本接口、严格 SemVer 历史绑定、架构和摘要校验及重启租约恢复）
  - `cargo test -p dockrev-api --locked --all-features snapshot_version_inference_binds_matching_observations_after_check_completion -- --test-threads=1`（通过；验证异步推断只绑定观测时相符的仓库与配置标签）
  - In `web/`: `bun run build`（264 tests passed、830 assertions）、`bun run lint`（0 errors；3 existing hook warnings）、`bun run build:demo:pages`
  - `python scripts/check-pwa-assets.py`、Storybook production build、rollback refresh race test 和 Storybook interaction test（421 stories passed）
  - TypeScript build、demo build、selected-version Spec contract check、visual-evidence document check、`cargo fmt --all -- --check` 和 `git diff --check` 均通过；相关旧版 service-detail Spec 保留历史格式，不通过当前 canonical-format checker。
- Rollout facts: 新安装和升级数据库均从空的历史归属表开始，只积累今后成功检查产生的观察。

## Empirical Acceptance

- Scenario: 在隔离 Compose 服务上以 D34 为当前摘要，将 Registry 的 `latest` 指向 D37 并完成成功检查；D37 预检分类为普通更新，指定版本任务按其观测摘要部署。随后将 `latest` 指向新的 D38 测试摘要，通过真实的计划检查和自动策略任务推进到 D38。
- Result: D34、D37、D38 摘要分别为 `sha256:0e80d1e99156a80a97d312a660b9881846d8aa29ed29b251ede3e9849458b6f3`、`sha256:62cd1070e44c09600683a821be574ff912827af153a90980913c6b6262d6df99`、`sha256:dd37582deb8ba457bcdc605050a4a990cc1956f7b2f2d4d3e816e79992279dad`。D37 预检分类为普通更新，任务 `job_01M3RW858800GVPWKSHCKSJJG4` 成功从 D34 更新至 D37，`skipTargetTagPull=true` 且 `pullTags=[]`；运行镜像和本地 `latest` 均指向 D37，Compose 文件未改变。计划检查 `chk_01M3RWCK71J71JG76263W0MRNS` 成功观察 D38，自动策略任务 `job_01M3RWCPB42YS39KDE92AKF169` 成功将服务从 D37 推进至 D38。最终运行镜像和本地 `latest` 均指向 D38；Compose 文件 SHA-256 在指定版本部署前、部署后和自动更新后均为 `f81eaa19847b54e8d23f123cd65454eca29c10165ec3317c32d329aeabc0a73c`。
- Evidence: 当前候选的 Docker/Compose 行为验收、API 与任务状态、观测记录和 Compose 摘要保存在 `/srv/codex/agents/01a0d6f3-3124-7432-9daa-99fd4bfb3755/validation-repair-4bafb473/a4-current/evidence`。计划和服务自动更新设置已恢复；测试 API、Compose 项目和 Registry 已停止，证据保留在 Agent Directory。
- Selected-version API and async version-inference logs, full workspace Clippy and test logs, and Storybook build/test logs are preserved under `/srv/codex/agents/01a0d6f3-3124-7432-9daa-99fd4bfb3755/validation-repair-4bafb473/`.
- Test note: 自动策略成功部署 D38；隔离 Registry 未提供派生兼容标签 `2.71.38`，因此兼容标签拉取产生非致命告警，基于配置标签 `latest` 的摘要拉取和服务更新仍成功。测试 Registry 使用任务自有 TLS CA；验证构建临时启用了 Reqwest native-root feature，仓库依赖配置和正式运行时 TLS 配置未改变。D34、D37、D38 均为隔离 Registry 内带 Linux/amd64 元数据的 BusyBox 测试镜像，不代表公开 Registry 镜像。
- Version baseline: 当前 `main` 与 root `VERSION` 为已发布的 `0.81.1`；按仓库手动版本流程，下一个 patch 目标为 `0.81.2`。本功能 PR 保持源码版本文件不变，由发布准备流程生成下一版本身份。
- Candidate binding: 最终候选 SHA、验收合同摘要、必需场景摘要和最终运行日志位置保存在当前交付流的 Candidate evidence card 中。

## Related Changes

- None

## References

- `./SPEC.md`
- `./HISTORY.md`
