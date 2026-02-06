# Dockrev API: resolvedTag 推测兼容 runtime platform digest（#n2z72）

## 状态

- Status: 待实现
- Created: 2026-02-06
- Last: 2026-02-06

## 背景 / 问题陈述

- 现象：线上环境仍会出现 `? → <candidate>`，无法推测“当前版本号”（`resolvedTag`）。
- 影响：浮动 tag（`latest` / `major.minor` / `*-alpine` 等）无法展示 `tag ≈ <semver>`，并且可能误判“有更新”。

## 根因（假设，将在实现中用测试验证）

- registry `GET /manifests/<tag>` 返回的 digest（`Docker-Content-Digest`）在 multi-arch 场景通常是 **index/manifest-list digest**；
- Docker runtime 从 `.RepoDigests` 采集到的 digest 在不同环境中可能是 **index digest** 或 **平台子 manifest digest**；
- 现有逻辑只用单一 digest 做等值比较，导致其中一种口径下匹配失败，从而 `resolvedTag` 无法产出。

## 目标 / 非目标

### Goals

- `resolvedTag` 推测在两种 runtime digest 口径下都可工作：
  - runtime digest == index digest
  - runtime digest == platform digest（host platform 对应的子 manifest digest）
- “no update fast-path” 在 runtime digest 已知时，不应因为 digest 口径差异而误报更新。

### Non-goals

- 不改变“runtime digest 不唯一（多 digest）则降级不推测”的策略。
- 不改变 candidates 选择策略（只修复 digest 对齐与比较）。

## 验收标准（Acceptance Criteria）

- Given multi-arch 镜像 + 浮动 tag（如 `latest`），且 runtime digest 为 platform digest，
  When 执行一次 check，
  Then API 返回 `services[].image.resolvedTag`（非空），UI 不再显示 `?`。

- Given runtime digest 已知且候选 tag 的 digest 与当前运行 digest 等价（index/platform 任一匹配），
  When 执行一次 check，
  Then 该服务不应保留候选版本（视为无更新）。

## 测试与验证（Testing）

- `cargo test -p dockrev-api`
- 新增/补齐单测覆盖：
  - registry manifest 同时暴露 index digest + platform digest（用于匹配）；
  - resolvedTag 推测可用 platform digest 命中；
  - no update fast-path 可用 platform digest 清除候选。

