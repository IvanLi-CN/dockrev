# 服务生命周期 HTTP 契约

## `GET /api/services/{serviceId}/lifecycle-status`

Response `200 OK`:

```json
{
  "state": "running",
  "activeJob": {
    "id": "job-service-lifecycle-123",
    "type": "service_lifecycle",
    "status": "running",
    "action": "restart"
  },
  "unavailableReason": null
}
```

- `state` 是 `running | stopped | partial | unknown`。
- `activeJob` 为 `null` 或阻塞此服务操作的 queued/running update、rollback、service_lifecycle 任务。
- `unavailableReason` 只在 `partial`、`unknown` 或发现活动任务时提供可展示原因。

## `POST /api/services/{serviceId}/lifecycle`

Request:

```json
{ "action": "start" }
```

Response `200 OK`:

```json
{ "jobId": "job-service-lifecycle-123" }
```

Conflict `409 Conflict`:

```json
{
  "reason": "service_operation_in_progress",
  "existingJobId": "job-update-456"
}
```

- 服务不存在返回 `404`。
- Dockrev 自身服务、`partial`、`unknown` 或与动作不相容的实时状态返回 `409`，包含稳定的 `reason`。
