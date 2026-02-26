---
title: 运维手册
description: Dockrev 生产运维、备份恢复与升级回滚。
---

# 运维手册

## 日常巡检（建议每天）

1. 检查 `/api/health` 与 `/supervisor/health`。
2. 查看 Queue 是否存在长时间 `running` 任务。
3. 抽查最近一次 discovery/check/update 日志是否异常。
4. 若启用 GHCR webhook，确认最近 delivery 无连续失败。

## 运行健康检查

- API 健康：`GET /api/health`
- 版本信息：`GET /api/version`
- Deploy 预检：`GET /api/deploy-check/report`
- Supervisor 健康：`GET /supervisor/health`

## 备份策略

建议至少备份：

1. SQLite 数据文件（`DOCKREV_DB_PATH`）
2. Supervisor 状态文件（`DOCKREV_SUPERVISOR_STATE_PATH`）
3. 部署 compose 与环境配置

## 备份执行建议

- 低峰期执行
- 先暂停高风险更新任务
- 备份后做一次快速恢复演练

示例（按你的路径调整）：

```bash
cp /data/dockrev.sqlite3 /backup/dockrev.sqlite3.$(date +%F-%H%M%S)
cp /data/supervisor/self-upgrade.json /backup/self-upgrade.json.$(date +%F-%H%M%S)
```

## 恢复流程

1. 停止 Dockrev 与 Supervisor
2. 恢复 DB 与状态文件
3. 启动服务
4. 通过 Queue 与 Overview 核对状态一致性

## 升级与回滚策略

- 常规升级：更新镜像 tag 后 `docker compose up -d`
- 自升级：通过 supervisor API 执行 apply 或 rollback
- 失败回滚：优先恢复到前一稳定镜像 + 最近可用 DB 备份

最小回滚步骤：

1. 切回上一稳定镜像 tag。
2. `docker compose up -d`。
3. 恢复最近可用 DB/state 备份。
4. 验证 `/api/version` 与关键页面功能。

## 观测建议

- 保留关键 job 日志（check/update/discovery）
- 监控 401、409、500 的异常峰值
- 监控 GHCR webhook 的 delivery 去重行为

## 共享测试机回归（可选）

仓库提供 `scripts/testbox/*.e2e.ts`，用于在共享测试机验证关键行为：

- check 并发与恢复
- version inference SSE 连续性
