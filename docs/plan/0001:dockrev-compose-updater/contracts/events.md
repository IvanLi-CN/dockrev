# 事件（Events）

本计划的“事件”用于后端内部队列与通知解耦（internal）。实现层面可用 DB 轮询/内存队列/通道等方式，但事件形状需要稳定，便于日志、审计与重放。

## job.created

- 范围（Scope）: internal
- 变更（Change）: New
- 生产者（Producer）: Job service
- 消费者（Consumers）: Worker / Notifier
- 投递语义（Delivery semantics）: at-least-once, ordering per `jobId`, retry with backoff, no DLQ in MVP

### 载荷（Payload）

```json
{
  "jobId": "job_...",
  "type": "check|update|rollback",
  "scope": "service|stack|all",
  "createdAt": "2026-01-18T00:00:00Z",
  "actor": "forward-user|webhook",
  "reason": "ui|webhook|schedule"
}
```

## job.updated

- 范围（Scope）: internal
- 变更（Change）: New
- 生产者（Producer）: Worker
- 消费者（Consumers）: Notifier / UI poller
- 投递语义（Delivery semantics）: at-least-once, last-write-wins

### 载荷（Payload）

```json
{
  "jobId": "job_...",
  "status": "queued|running|success|failed|rolled_back",
  "ts": "2026-01-18T00:00:00Z",
  "summary": {
    "changedServices": 1,
    "oldDigests": { "svc_...": "sha256:..." },
    "newDigests": { "svc_...": "sha256:..." },
    "backup": {
      "status": "skipped|success|failed",
      "artifactPath": "/data/backups/stk_.../20260118-000000Z.tar.zst",
      "sizeBytes": 123456,
      "skippedTargets": [
        { "target": "docker-volume:app_db_data", "reason": "skipped_by_size" }
      ]
    }
  }
}
```

### 兼容性规则（Compatibility rules）

- 只允许新增字段（additive），不得删除或重命名字段（需要弃用周期）。
