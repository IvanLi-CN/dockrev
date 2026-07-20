# Dockrev: 运行态版本漂移自动发现（runtime diff scan + SSE）（#8xt2t）

## 状态

- Status: 已完成
- Created: 2026-02-17
- Last: 2026-07-20
- Legacy source: `docs/plan/8xt2t:runtime-drift-scan-sse/PLAN.md` pending delete approval

## 背景 / 问题陈述

- 外部操作（`docker compose pull/up`、supervisor 自升级、手工替换镜像等）会让真实运行态镜像 digest 与 Dockrev DB 缓存发生漂移。
- Dockrev 需要轻量对账运行态，而不是靠提高全量检查频率掩盖问题。
- 对 `latest` 等 moving tag，多个 stack 共用同一 `repo:tag` 时，宿主机本地 tag 可能已被其中一个 stack 更新到新镜像；未重建的 stack 仍然运行旧容器镜像。
- 因此 `services.current_digest` 的真相源必须是运行中容器实际绑定的 image ID / digest；本地 `repo:tag` 当前解析值只能用于候选发现或展示补充，不能覆盖已观测运行态。

## Goals

- 后端按计划执行 runtime diff scan，对比运行态 digest 与 DB `current_digest`，发现漂移时自动修正当前 digest、resolved tag 与候选状态。
- 前端保留 runtime scan 结果的事件可见性，但只读页面打开时不得自动触发 runtime scan；自动漂移发现依赖后台定时 scan，显式操作如 `POST /api/runtime-scans` 仍可按需触发。
- check 与 runtime scan 复用同一套候选计算与 resolved tag 推断策略。
- 当运行容器对应 image 无法提供匹配 repo 的 `RepoDigests` 时，使用容器 `.Image` 的 immutable image ID 作为运行态兜底，避免 moving tag 污染 `current_digest`。

## Non-goals

- 不把 runtime scan 变成新的全量慢 check。
- 不引入跨 tag 候选发现策略。
- 不修改 runtime scan / stacks / services API response shape。
- 不对线上数据库做自动 backfill；部署后由下一轮 check/runtime scan 自然纠正。

## 行为规格

- `POST /api/runtime-scans` 支持 `scope=all|stack|service`，创建 `runtime_scan` job 并按 scope 扫描 Docker Compose 项目。
- 只读页面（如概览、服务列表、服务详情）读取已有 DB 状态，不得在 mount/page-open 阶段隐式触发 `scope=all` 的 runtime scan。
- runtime scan 先按 compose project/service label 找到运行容器，再读取容器 `.Image` 和 `.State.StartedAt`。
- 对每个容器 image ID，优先用 `docker image inspect <image-id>` 的 `RepoDigests` 匹配服务 image repo；若唯一匹配到 digest，则该 digest 为运行态 digest。
- 若没有匹配 repo digest，但容器 `.Image` 非空，则以该 image ID 作为运行态兜底；多副本兜底值不唯一时保持未知，不写入混乱状态。
- 已知运行态 digest 后，registry manifest 查询只用于计算同 tag candidate；候选与当前相等时继续走既有 no-op fast path。
- `checked_at` / stack `last_check_at` 随 runtime scan 刷新，便于 UI 展示数据新鲜度。

## 验收标准

- Given 服务容器被外部更新，When runtime scan 运行，Then DB `current_digest` 与 UI 服务详情更新为运行态 digest，并重新计算候选。
- Given check 与 runtime scan 拿到相同 runtime digest 与 registry tags，When 分别执行，Then resolved tag / resolved tags 结果一致。
- Given `trtff-api` 仍运行旧 image ID，而同宿主机 `ghcr.io/sequenxe/trtff:latest` 已被其他 stack 拉到新 image，When runtime scan 运行，Then `trtff-api.current_digest` 保留旧 image ID，`candidate_digest` 指向最新 registry digest。
- Given `ctp-recorder` 已运行新 digest，When 同轮 runtime scan 运行，Then 它的 `current_digest` 为新 digest 且不产生同 digest candidate。
- Given 用户仅打开概览、服务列表或服务详情页，When 页面完成首屏加载，Then 不会因为 page-open/mount 自动创建新的 `runtime_scan` job。

## 验证

- `cargo test -p dockrev-api runtime_scan`
- `cargo test -p dockrev-api runtime_scan_keeps_container_image_id_when_shared_moving_tag_was_pulled_elsewhere`
- `cargo fmt --all --check`

## 参考

- `crates/dockrev-api/src/runtime_scan.rs`
- `crates/dockrev-api/src/api/operations.rs`
- `docs/plan/8xt2t:runtime-drift-scan-sse/PLAN.md`
