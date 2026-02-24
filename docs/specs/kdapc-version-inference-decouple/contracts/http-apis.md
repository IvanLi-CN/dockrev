## `GET /api/stacks/{stack_id}`

`services[]` 新增字段：

```json
{
  "versionInference": {
    "status": "ready | pending",
    "reason": "cache_miss | cache_stale | all_failed | new_version | force | running | not_required",
    "checkedAt": "RFC3339 | null"
  }
}
```

说明：

- `pending` 表示当前镜像存在新的推测任务进行中（即使已有旧缓存也视为未就绪）。
- `ready` 表示当前可读到稳定缓存，或该服务不需要推测。

## `POST /api/services/{service_id}/version-inference/refresh`

- 作用：强制触发该服务对应镜像的版本推测采集任务。
- Request body: `{}`（保留空对象）
- Response: `202 Accepted`

示例响应：

```json
{
  "status": "pending",
  "serviceId": "svc_xxx",
  "imageRepo": "ghcr.io/acme/web",
  "reason": "force"
}
```
