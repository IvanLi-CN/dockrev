---
title: 术语表
description: Dockrev 文档中的核心术语解释。
---

# 术语表

- **Discovery**：扫描 Docker Compose 项目并同步到 Dockrev 的过程。
- **Check**：检查服务当前镜像与候选版本的过程。
- **Update / Apply**：执行实际更新动作并重建/重启目标服务。
- **Dry-run**：仅做预检查，不执行真实更新。
- **Job**：系统内一次异步任务执行记录。
- **Scope**：任务作用范围（all / stack / service）。
- **Resolved Tag**：由 digest 反推得到的候选版本标签。
- **Supervisor**：Dockrev 自升级执行器与控制台。
- **Forward Auth**：由入口代理完成认证并向 Dockrev 透传可信用户/组头；Dockrev 再据此执行项目侧鉴权。
- **GHCR Webhook**：GitHub Packages 事件回调，触发自动扫描。
