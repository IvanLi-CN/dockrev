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
- 指标持久化由独立的 `MetricsStore` 负责：`metrics.sqlite3` 持有 raw/latest/rollups，主库仅持有业务状态和迁移状态。首次启动复制旧表并用有序行哈希与行数校验后才切换；导入 raw 记录稳定签名，GC 在清理 legacy raw 前记录墓碑。latest 以来源标记区分导入投影、运行时采样和无来源的旧行，恢复时重建并验证导入投影，但保留更新鲜的运行时值与未知旧行；迟到的 raw 会同步更新当前 native latest 的前驱计数，避免首页速率使用过期基线。rollup 的行级完整性指纹与当前/受信行数元数据让已验证重启只做读校验，缺失或篡改才重建；行数由写入触发器增量维护，采样与 GC 不扫描整个 rollup 表。源 raw 修订变化时，迁移在清空旧 legacy raw 前记录其桶集合并重建这些桶，因此已从源删除的 legacy 历史不会遗留在长窗口响应中，同时不触碰没有 legacy 来源的 native 长窗口桶。旧文件升级时从已留存桶补写指纹，不依赖已过期 raw 重建长窗口桶。因此派生读模型损坏、陈旧或回退的导入 latest 可恢复，不复活已清理旧行，也不删除超出 raw 留存期的 active latest 或长窗口桶；采样周期对所有成功项目只提交一次指标事务。
- 迁移 manifest 同时记录 legacy raw 与 latest 的源计数/哈希、raw 最大 id 及源变更 revision。主库的 source revision 仅在旧指标表变更时递增，所以健康重启避免 raw、rollup 和墓碑全表扫描；指标库 target revision 只有在与受信 revision 不一致时才触发深度恢复。GC 墓碑另有活动 count/双校验和与受信快照，运行时 raw/latest 另以增量行数受信快照保护；未受信 native 数据不得猜测修复。深度恢复先比对快照再交叉验证墓碑仍对应主库源行；source latest 变化保留已验证墓碑，source raw 在已裁剪 legacy raw 后变化则阻断启动。任何已裁剪 native 或 legacy raw 后无法证明完整的长窗口桶同样阻断启动而非标记为可信。旧 manifest 缺少新字段时按未完成迁移处理。
- 全局更新跟踪和概览的 compact jobs 路径经 SQLite JSON 投影只读取派生字段，不把完整 `summary_json` 选入 Rust；兼容的默认 jobs 路径仍读取原始 summary。
- 资源历史短窗口保持原始样本契约，`7d` 与 `30d` 从 1 分钟或 5 分钟读模型返回均值主线及对齐峰值。首页和资源概览并行读取指标库与主库的 query-only 读模型，在 Rust 内按 service id 合并并过滤孤儿指标；镜像快照由启动预热、30 分钟后台协调器与显式操作刷新，GET 不排队工作。
- 监控的 SSE、数据库 schema 与设置 wire shape 保持兼容；REST 资源历史和 Overview 聚合新增 `7d`/`30d` 窗口，前端资源面板提供相应控制项。控制面退化时允许当前样本缺失并由后续 cadence 自动恢复。

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
