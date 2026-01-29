# 数据库（DB）

## 保存推测出的 `resolvedTag(s)`

- 范围（Scope）: internal
- 变更（Change）: Modify
- 影响表（Affected tables）: `services`

### Schema delta（结构变更）

新增列（均可为空）：

- `services.current_resolved_tag TEXT`
- `services.current_resolved_tags_json TEXT`（JSON string of `string[]`）

### Migration notes（迁移说明）

- 向后兼容窗口（Backward compatibility window）: 永久向后兼容（新增 nullable 列）。
- 发布/上线步骤（Rollout steps）:
  - 先发布包含新列的迁移/自检逻辑；
  - 再发布写入与读取逻辑（旧数据保持 null）。
- 回滚策略（Rollback strategy）:
  - 回滚到旧版本时忽略新增列（SQLite 允许存在未使用列）。
- 回填/数据迁移（Backfill / data migration, 如适用）:
  - 无需回填；由下一次 check 任务自然写入。

