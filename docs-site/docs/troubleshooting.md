---
title: 故障排查
description: Dockrev 常见问题的定位路径与修复建议。
---

# 故障排查

## 1) 页面显示 401 / 无法进入功能页

排查项：

- 是否注入了 `DOCKREV_AUTH_FORWARD_HEADER_NAME` 指定的用户头
- 如启用组鉴权，是否同时注入了 `DOCKREV_AUTH_GROUP_HEADER_NAME` 指定的组头
- `DOCKREV_AUTH_ALLOWED_USER` / `DOCKREV_AUTH_ALLOWED_GROUP` 是否与当前身份匹配
- 生产环境是否误开启匿名开关

立即处理：

1. 确认 `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV=false`（生产）。
2. 在网关补齐 `DOCKREV_AUTH_FORWARD_HEADER_NAME`（默认 `X-Forwarded-User`），必要时再补齐 `DOCKREV_AUTH_GROUP_HEADER_NAME`（默认 `Remote-Groups`）。
3. 确认 `DOCKREV_AUTH_ALLOWED_USER` / `DOCKREV_AUTH_ALLOWED_GROUP` 至少配置一个，且与当前身份命中其一。
4. 刷新后若仍 401，查看网关与 Dockrev 日志中的请求头透传情况。

## 2) 自动发现不到 compose 项目

排查项：

- 容器是否包含 `com.docker.compose.project` 与 `config_files` 标签
- `config_files` 绝对路径是否在 dockrev 容器内同路径可读
- 是否存在 Dockrev 自生成的 override 文件未挂载，例如 `self-upgrade.override.yml` 或 `/tmp/dockrev-override-*.yml`
- 用户自己维护的额外 compose / override 文件是否仍然存在且已挂载

立即处理：

1. 在宿主机检查容器标签是否存在 compose 信息。
2. 确认 `config_files` 路径已“同绝对路径只读挂载”到 Dockrev 容器。
3. 若唯一缺失文件是 Dockrev 自生成的 override 文件（如 `self-upgrade.override.yml` 或 `/tmp/dockrev-override-*.yml`），重跑 discovery scan，Dockrev 会回退到仍可读的稳定 compose 文件。
4. 若缺失的是你自己维护的 compose / override 文件，仍会被判为 invalid；先恢复该文件或修复同绝对路径挂载，再重跑 discovery。

## 3) Check 频繁失败或变慢

排查项：

- registry 是否触发 `429`
- 重试参数是否过小
- 是否网络受限 / 凭据失效

立即处理：

1. 先确认镜像仓库凭据是否过期。
2. 遇到 `429` 时，提高 `DOCKREV_REGISTRY_RETRY_MAX_ATTEMPTS` 与 `MAX_MS`。
3. 对单服务执行 check，对比全量 check 判断是否为局部仓库问题。

## 4) GHCR webhook 未触发扫描

排查项：

- 回调 URL 是否公网 HTTPS 可达
- GitHub webhook delivery 是否到达
- 签名头 `X-Hub-Signature-256` 是否匹配
- repo 是否处于“已选中跟踪”状态
- Queue 中是否已经出现命中服务的 `check.service`（零命中才会回退到 discovery）

立即处理：

1. 在设置页确认：已启用、PAT 已保存（掩码可见）、已跟踪仓库数 > 0。
2. 同步 webhook 后确认结果是 `created/updated/noop`，无 `error/conflict`。
3. 在 GitHub 仓库 Webhooks 查看最近 delivery，检查状态码与响应体。
4. 若 delivery 已返回 `200` 但 candidate 没刷新，查看对应 `check.service` job logs；升级后 digest-only 服务记录也应被正常检查，而不是报 `invalid image ref`。

## 5) 自升级按钮不可用

排查项：

- `/supervisor/self-upgrade` 是否可达
- Forward Auth 是否同时传到 supervisor 路径
- `DOCKREV_SUPERVISOR_TARGET_IMAGE_REPO` 是否配置正确

立即处理：

1. 直接请求 `/supervisor/self-upgrade`，确认返回非 401。
2. 若 401，仅修复 supervisor 路径的 Forward Auth 透传。
3. 校验 `DOCKREV_IMAGE_REPO` 与 `DOCKREV_SUPERVISOR_TARGET_IMAGE_REPO` 是否一致。

## 6) Job 卡住 running

排查项：

- Queue 中是否存在同 scope 互斥任务
- 重启后是否自动恢复为 failed（startup recovery）
- 查看 job logs 判断阻塞阶段

立即处理：

1. 先看是否存在同 scope 互斥任务未结束。
2. 对卡住任务记录日志后重启服务，观察是否按恢复策略转为 failed。
3. 若持续复现，保留 job logs 并按 scope 缩小到单服务复测。
