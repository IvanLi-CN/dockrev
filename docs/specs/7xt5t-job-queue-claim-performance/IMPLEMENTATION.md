# Dockrev：Jobs 队列领取索引与慢告警 实现状态（#7xt5t）

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实，避免这些细节散落到 PR / Git 历史里。

## Current Status

- Implementation: 已实现，待 PR
- Lifecycle: active
- Catalog note: 本地 Rust 质量门禁、发布契约检查和规格 drift 检查已通过。

## Coverage / rollout summary

- 101 只读基线确认了空闲轮询会触发 `jobs` 全表扫描和临时排序。
- `idx_jobs_type_status_created_at_id` 已加入幂等 schema 初始化，领取 SQL 与 `EXPLAIN QUERY PLAN` 测试共用同一常量。
- 成功且超过 25 ms 的领取操作会按 `job_type` 限频 60 秒输出结构化 WARN。
- 发布后需复核新索引、查询计划、空闲 CPU 与慢领取 WARN。

## Remaining Gaps

- 完成 PR review、required checks 与发布后只读验证。

## Related Changes

- `crates/dockrev-api/src/db/schema.rs`
- `crates/dockrev-api/src/db/jobs.rs`
- `crates/dockrev-api/src/db/jobs_tests.rs`

## References

- `./SPEC.md`
- `./HISTORY.md`
