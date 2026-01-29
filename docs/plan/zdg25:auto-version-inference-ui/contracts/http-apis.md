# HTTP API

## Stack detail/list: `services[].image` 增加 `resolvedTag(s)`

- 范围（Scope）: external
- 变更（Change）: Modify
- 鉴权（Auth）: 继承现有鉴权策略（不新增权限范围）

### 受影响端点（Endpoints）

（以下端点的 `services[].image` 结构一致，新增字段相同。）

- `GET /api/stacks`（stack list）
- `GET /api/stacks/:stackId`（stack detail）

### 响应（Response）

`services[].image` 新增可选字段（向后兼容）：

- `resolvedTag: string | null`：当 `tag` 为 floating tag（例如 `latest`）且推测成功时，返回一个 semver tag（建议保留原 tag 前缀，例如 `v0.2.9` 或 `0.2.9`，与 registry 实际 tag 一致）。
- `resolvedTags: string[] | null`：用于表达“多个 semver tag 指向同一 digest”的情况；当存在多个匹配时返回所有匹配 tag（按 semver 从高到低；稳定版优先于 pre-release），否则可为 null。

补充口径（用于实现与测试对齐）：

- `digest: string | null`（既有字段）：表示 **运行中容器 digest**，仅在可唯一确定单一 digest 时返回；无法获取或多 digest 并存时返回 null（或等价降级）。

其余字段保持不变（示意）：

```json
{
  "ref": "ghcr.io/org/app:latest",
  "tag": "latest",
  "digest": "sha256:...",
  "resolvedTag": "v0.2.9",
  "resolvedTags": ["v0.2.9"]
}
```

### 兼容性与迁移（Compatibility / migration）

- 新字段均为可选：旧 Web/外部客户端忽略即可。
- 若推断失败或运行态不可用：返回 null（或省略字段），不得返回猜测值。
