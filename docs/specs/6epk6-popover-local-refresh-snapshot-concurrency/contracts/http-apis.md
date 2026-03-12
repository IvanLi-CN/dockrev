## `POST /api/services/{service_id}/version-inference/refresh`

- 作用：强制刷新该 service 下**指定 digest** 的 digest-tags snapshot。
- Request body:

```json
{
  "digest": "sha256:..."
}
```

- 约束：`digest` 必须等于该 service 当前的 `current_digest` 或 `candidate_digest`；否则返回 `404`。
- Response: `202 Accepted`

示例响应：

```json
{
  "status": "pending",
  "serviceId": "svc_xxx",
  "imageRepo": "ghcr.io/acme/web",
  "digest": "sha256:...",
  "reason": "force"
}
```

说明：

- `reason=force`：本次成功 enqueue 目标 digest 的 snapshot task。
- `reason=running`：目标 digest 的 snapshot task 已在队列或运行中，本次请求复用已有任务。
- 该接口不再具备“同时刷新 current + candidate”的 service 级语义。

## `GET /api/services/{service_id}/digest-tags-snapshot`

- 输入：query `digest=<sha256:...>`。
- 作用：读取该 digest 的 snapshot；若目标 digest 当前存在 in-flight task，则优先返回 `pending`。

`pending` 响应：

```json
{
  "status": "pending",
  "digest": "sha256:...",
  "retryAfterMs": 800
}
```

`ready` 响应保持既有结构：

```json
{
  "digest": "sha256:...",
  "tags": ["v1.2.3", "1.2.3"],
  "checkedAt": "2026-03-10T12:34:56Z",
  "scan": {
    "repoTagsTotal": 42,
    "repoTagsConsidered": 40,
    "manifestsOk": 40,
    "manifestsTimeout": 0,
    "manifestsError": 0
  }
}
```
