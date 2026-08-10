# 服务树上下文操作 HTTP 契约

## Stack lifecycle status

`GET /api/stacks/{stackId}/lifecycle-status`

成功返回：

```json
{
  "state": "running",
  "unavailableReason": null,
  "activeJob": null
}
```

- `state`: `running | stopped | partial | unknown`。
- `unavailableReason`: 自托管、归档、Compose 配置/服务定义缺失或查询失败原因；Compose 能力不足通过写接口的 `compose_v2_required` 返回。
- `activeJob`: 首个会阻塞 Stack 生命周期的 queued/running update、rollback、service lifecycle 或 stack lifecycle 任务。

## Stack lifecycle trigger

`POST /api/stacks/{stackId}/lifecycle`

```json
{ "action": "start" }
```

- `action`: `start | stop | restart`。
- 成功：`200 { "jobId": "..." }`。
- 目标不存在：`404`；归档、自托管或状态不兼容：`409`；活动任务冲突：`409` 并返回 `existingJobId`。
- 创建任务类型 `stack_lifecycle`，`scope=stack`，任务记录携带 `stackId`，摘要携带 `stackName`、`action` 与实际服务 ID 集合。

## Service lifecycle compatibility

- `GET/POST /api/services/{serviceId}/lifecycle(-status)` 必须把覆盖该服务的活动 `stack_lifecycle` 视为冲突。
- apply update、rollback 和 service lifecycle 的预约逻辑必须反向识别覆盖目标服务的活动 `stack_lifecycle`。
