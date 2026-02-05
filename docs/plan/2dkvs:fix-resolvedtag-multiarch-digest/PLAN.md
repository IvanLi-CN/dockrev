# Dockrev API: 修复 multi-arch 镜像的当前版本推测（resolvedTag）与 digest 对齐（#2dkvs）

## 状态

- Status: 已完成
- Created: 2026-02-05
- Last: 2026-02-05

## 背景 / 问题陈述

- 现象：Web UI 中大量服务在 Versions 列显示 `? → <candidate>`，无法推测“当前版本号”（resolvedTag）。
- 预期：当 Compose 使用 `latest` / `18-alpine` / `4.39` 等非严格 semver tag 时，Dockrev 能基于运行态 digest 反推出一个可解释的版本 tag（resolvedTag），用于 UI 展示与状态判断。

## 根因（Root cause）

- Docker 运行态采集到的 `RepoDigests` 通常是 **multi-arch index/manifest-list digest**。
- 现有 registry manifest 解析逻辑在 multi-arch 场景优先返回 **host platform 子 manifest digest**。
- 两类 digest 口径不一致，导致 `digest(tag) == runtime_digest` 的匹配永远失败，从而 `resolvedTag/resolvedTags` 无法产出（等同一直为 null）。

## 目标 / 非目标

### Goals

- 统一 digest 口径：优先使用 registry 返回的 `Docker-Content-Digest`（即 tag 对应的 manifest/index digest），以便与 Docker 运行态 `RepoDigests` 对齐。
- 在 digest 对齐后，`resolvedTag` 推测在 multi-arch 镜像上可正常工作（当存在同 digest 的可解析版本 tag 时）。

### Non-goals

- 不在本计划中改变 candidates 选择策略（例如对 `latest` 的 semver 候选排序）；仅修复当前版本推测与 digest 对齐。

## 验收标准（Acceptance Criteria）

- Given 一个 multi-arch 镜像服务使用 `latest`（或 `major.minor` / `*-alpine` 等非严格 semver tag），且 registry 中存在某个可解析版本 tag 与其指向同一 digest，
  When 执行一次 check，
  Then `/api/stacks/:id` 返回 `services[].image.resolvedTag`（非空），Web UI 不再显示 `?`。

## 测试与验证（Testing）

- `cargo test -p dockrev-api`
- 单测覆盖：registry manifest 解析在 multi-arch manifest-list 下应优先返回 header digest（而非子 manifest digest）。

## 风险与回滚（Risks）

- 风险：candidate/current 的 digest 展示值会从 “子 manifest digest” 变为 “index digest”。这是口径变化，但 index digest 可用于 `repo@sha256:<digest>` 拉取且跨平台一致，预期更符合实际。

## 交付物（Deliverables）

- PR #57
