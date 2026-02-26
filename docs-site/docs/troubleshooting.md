---
title: 故障排查
description: Dockrev 常见问题的定位路径与修复建议。
---

# 故障排查

## 1) 页面显示 401 / 无法进入功能页

排查项：

- 是否注入了 `DOCKREV_AUTH_FORWARD_HEADER_NAME` 指定的头
- 生产环境是否误开启匿名开关
- 反向代理是否丢失了透传头

## 2) 自动发现不到 compose 项目

排查项：

- 容器是否包含 `com.docker.compose.project` 与 `config_files` 标签
- `config_files` 绝对路径是否在 dockrev 容器内同路径可读
- 是否存在 self-upgrade 生成的 override 文件未挂载

## 3) Check 频繁失败或变慢

排查项：

- registry 是否触发 `429`
- 重试参数是否过小
- 是否网络受限 / 凭据失效

## 4) GHCR webhook 未触发扫描

排查项：

- 回调 URL 是否公网 HTTPS 可达
- GitHub webhook delivery 是否到达
- 签名头 `X-Hub-Signature-256` 是否匹配
- repo 是否处于“已选中跟踪”状态

## 5) 自升级按钮不可用

排查项：

- `/supervisor/self-upgrade` 是否可达
- forward header 是否同时传到 supervisor 路径
- `DOCKREV_SUPERVISOR_TARGET_IMAGE_REPO` 是否配置正确

## 6) Job 卡住 running

排查项：

- Queue 中是否存在同 scope 互斥任务
- 重启后是否自动恢复为 failed（startup recovery）
- 查看 job logs 判断阻塞阶段
