# Supervisor state file

目标：自我升级过程可恢复（页面刷新/短暂断连/进程重启后仍能读取最新状态）。

## Path

- 默认：`./data/supervisor/self-upgrade.json`
- 要求：写入必须是原子替换（write temp + rename），避免半写入损坏。

## Schema (JSON)

```json
{
  "schemaVersion": 1,
  "opId": "sup_...",
  "state": "idle|running|succeeded|failed|rolled_back",
  "target": { "tag": "1.2.3", "digest": "sha256:..." },
  "previous": { "tag": "1.2.2", "digest": "sha256:..." },
  "startedAt": "2026-01-24T00:00:00Z",
  "updatedAt": "2026-01-24T00:00:00Z",
  "progress": { "step": "precheck|pull|apply|wait_healthy|postcheck|rollback|done", "message": "..." },
  "logs": [
    { "ts": "2026-01-24T00:00:00Z", "level": "INFO|WARN|ERROR", "msg": "..." }
  ]
}
```

## Compatibility

- 仅允许追加字段；不删除/重命名既有字段。
- `schemaVersion` 变更需要提供读写兼容窗口与迁移策略。
