---
title: 常见问题
description: Dockrev 常见提问与最佳实践。
---

# 常见问题

## Dockrev 能管理哪些部署形态？

主要针对 Docker Compose 项目。系统通过 Compose labels 自动发现服务。

## 必须使用 GHCR 吗？

不是。Dockrev 可读取通用 registry 信息；GHCR webhook 是增量触发能力。

## 为什么我在 Settings 看不到 PAT 明文？

安全设计：后端只返回掩码（例如 PAT 的 `******` 或部分密钥的圆点掩码），防止凭据回显泄漏。

## 什么时候需要 `DOCKREV_IMAGE_REPO`？

当你希望 UI 正确识别“Dockrev 自身服务”并显示“升级 Dockrev”入口时。

## 可以把 docs/plan 与 docs/specs 作为用户文档吗？

不建议。这两类目录是工程内部规格资产，不是稳定用户手册。

## 是否支持自动发布文档站？

支持。仓库内 `docs-pages` workflow 可自动发布到 GitHub Pages。
