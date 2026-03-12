# Dockrev：非 service update 的 semver pull 保留 OCI 原始 tag 并回退无 v 变体（#xyma9）

## 状态

- Status: 重新设计（#99egq）
- Created: 2026-03-10
- Last: 2026-03-10


## 说明

- 本规格已被 `#99egq` 重新设计；Dockrev 后续不再通过服务端猜测 semver tag，而改为由更新任务发起方显式提供 `targetTag` 与 `pullTags[]`。

## 背景 / 问题陈述

- `scope=stack/all` 更新会在主更新链路完成后，基于镜像 OCI label `org.opencontainers.image.version` 执行 `semver_pull`。
- 现有实现会把 `v0.9.14` 这类版本号规范化成 `0.9.14` 后直接执行 `docker pull repo:0.9.14`。
- 当 registry 只发布保留 `v` 前缀的 tag（如 `repo:v0.9.14`）时，更新已经成功切换到新 digest，job 仍会因为 `failureStep=semver_pull` 被误判为失败。

## 目标 / 非目标

### Goals

- 非 service update 的 `semver_pull` 优先保留 OCI label 的原始 tag，再回退到去掉 `v/V` 的规范化 tag。
- 保持现有 `sync configured tag -> semver_pull` 顺序、阻断失败语义与 summary 结构兼容。
- 让 `semverPulled` 记录实际成功拉取的 tag，允许带 `v` 前缀。

### Non-goals

- 不修改 `scope=service` 的显式 `targetTag + targetDigest` 契约。
- 不新增 registry 预查询接口、配置开关或数据库字段。
- 不调整 supervisor 自升级链路的对外契约。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/updater.rs`
- `docs/specs/README.md`
- 本规格文档与相关测试

### Out of scope

- `crates/dockrev-supervisor/**`
- `web/**`
- HTTP API / DB schema 变更

## 需求（Requirements）

### MUST

- 仅对非 service update 保持 `semver_pull` 行为；`scope=service` 继续完全跳过该分支。
- 从 OCI label 读取到的 raw version 必须先按“允许前导 `v/V`”校验为合法 semver，再生成 ordered candidates。
- ordered candidates 必须按 `[raw_tag_if_distinct, normalized_no_v_tag]` 顺序执行 pull，并对重复候选去重。
- 若本地 `RepoTags` 已经包含某个候选 tag，必须按候选顺序直接短路成功，不再为更早失败候选消耗远端 pull 重试预算。
- 同一 job 内若较晚候选（如 normalized tag）已在去重集合中，仍不得抢先跳过更早的 raw candidate；只有轮到该候选时才允许复用 job 级短路。
- 所有候选都失败时，job 继续以 `failureStep=semver_pull` 失败，错误信息需包含全部尝试过的 refs。

### SHOULD

- `semverPulled` 应记录首个成功的实际 tag ref，包含“本地 RepoTags 命中即短路成功”的路径，便于 `/queue/job` 与 summary 直接排障。
- 现有“tag 对齐先于 semver_pull”的顺序断言需保持稳定。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 非 service update：`pull -> up -> health/inspect -> sync configured tag -> ordered semver_pull -> success`。
- ordered semver_pull：
  - 先读取 OCI raw version；
  - 若 raw version 非空、无 build metadata，且在允许前导 `v/V` 的条件下可解析为 semver，则生成候选；
  - 候选 1 为 raw tag 原样保留；候选 2 为去掉前导 `v/V` 后的 normalized tag；
  - 若两个候选相同，仅保留一个；
  - 按顺序检查本地 `RepoTags` / 执行 `docker pull`，首个成功即结束。

### Edge cases / errors

- raw version 为空、`<no value>`、包含 build metadata 或无法解析为 semver 时，直接跳过 semver pull。
- raw tag pull 失败但 normalized tag 成功时，任务整体仍视为成功。
- raw 与 normalized 都失败时，失败信息需聚合两次尝试的 ref 与错误，避免只看到最后一次失败。

## 验收标准（Acceptance Criteria）

- Given OCI label 为 `v0.9.14` 且 registry 只发布 `repo:v0.9.14`，When 非 service update 完成，Then job 为 `success`，且 `semverPulled` 记录 `repo:v0.9.14`。
- Given raw `v` tag 不存在但 `repo:0.9.14` 存在，When 非 service update 完成，Then 会回退到 normalized tag 并成功。
- Given raw 与 normalized 都不存在，When job 结束，Then 状态仍为 `failed`，`failureStep=semver_pull`，且错误文本包含两次尝试的 refs。
- Given `scope=service` 更新，When job 完成，Then 不会重新进入 semver pull 分支。
- Given 非 service update 成功路径，When 检查命令顺序，Then `sync configured tag` 仍发生在 `semver_pull` 之前。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo test -p dockrev-api`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: `updater.rs` 生成 ordered semver candidates，并在 `semver_pull` 中按顺序尝试 raw/normalized tag。
- [x] M2: 补齐 raw success、fallback success、already-tagged short-circuit、dual failure 聚合错误与 service-scope guard 的回归测试。
- [x] M3: 完成验证与规格同步，更新索引状态。

## 风险 / 假设

- 假设：registry tag 是否存在继续通过 `docker pull` 本身判断，而不是额外做远端探测。
- 风险：若发布方同时保留大小写不同的 tag，raw-first 语义会优先信任 OCI label 的原始大小写。

## 变更记录（Change log）

- 2026-03-10: 新建规格，冻结“非 service update 的 semver pull 优先 raw tag，再回退 normalized tag”语义。
- 2026-03-10: 完成 `dockrev-api` 实现与回归测试，保持非 service update 的阻断失败语义不变。
- 2026-03-10: review-loop 补强本地 RepoTags 全候选短路、local-hit summary 记录与聚合失败尝试次数可观测性。
- 2026-03-10: 合并 `origin/main` 后补强 raw-first 去重顺序，确保先前已拉取 normalized tag 时仍会优先尝试 raw tag。
