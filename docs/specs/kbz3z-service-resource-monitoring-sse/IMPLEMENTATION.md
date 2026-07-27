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
- 单一 `ResourceSamplingCoordinator` 驱动历史周期，并以项目级 in-flight/cache 让活跃 SSE 复用同一次采集；每个历史周期只调用一次容器发现。
- Docker stats 使用 `stream=false&one-shot=true`，CPU 以同容器 ID 的前次原始计数计算；首样本为 `0`。基线保留窗口覆盖最长 300 秒 cadence，缺少 `system_cpu_usage` 的响应不安装基线。单项目 SSE 过滤发现只清理该项目的失效基线，全局发现会同步回收已删除项目的旧基线。协调器同步驱逐过期且非进行中的项目状态，单项目 SSE 继续通过 Compose label 过滤发现容器；拥有采集的 future 被取消时由 guard 清理 `in-flight` 并唤醒等待者，监控禁用期间将已开始的采集标记为失效并向等待者传播错误，不回填缓存。
- 原始样本固定保留 24 小时；独立 GC 任务在启动后与每分钟最多完成 10 个 10,000 行批次并在批间让出执行权，不执行自动 `VACUUM`，不阻塞历史采样主循环。
- 监控的 REST、SSE、数据库 schema、设置 wire shape 与前端控制项保持不变；控制面退化时允许当前样本缺失并由后续 cadence 自动恢复。

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
