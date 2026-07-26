# Dockrev：服务资源监控 实现状态（#kbz3z）

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实。

## Current Status

- Implementation: 已实现，待 PR
- Lifecycle: active
- Rollout: 本次不包含 101 Docker 恢复或生产部署。

## Coverage / rollout summary

- 普通命令 runner 在超时 future 被丢弃时终止子进程，避免 `docker ps` 等阻塞命令残留。
- 应用启动时只创建一个 Docker Engine client，并注入实时 SSE 与历史采样链路。
- 共享 client 实现 4 请求全局上限、连续两次可恢复失败熔断、5 秒至 60 秒退避和单半开探测；探测取消会安全地回到打开状态。
- 监控的 REST、SSE、数据库 schema、设置与前端契约保持不变；控制面退化时允许当前样本缺失并由后续 cadence 自动恢复。

## Remaining Gaps

- 合并与部署后，需要单独授权的 101 运行时恢复和只读验收。

## Related Changes

- `crates/dockrev-api/src/runner.rs`
- `crates/dockrev-api/src/docker_engine.rs`
- `crates/dockrev-api/src/resource_usage.rs`
- `crates/dockrev-api/src/main.rs`

## References

- `./SPEC.md`
- `./HISTORY.md`
