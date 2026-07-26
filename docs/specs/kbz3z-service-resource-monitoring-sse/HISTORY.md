# Dockrev：服务资源监控 演进历史（#kbz3z）

> 这里记录会影响后续维护者理解“为什么如此设计”的关键演进；规范正文仍以 `./SPEC.md` 为准。

## Decision Trace

- 2026-07-20: 历史采样采用每个 compose project 独立固定 cadence、single-flight 与跳过过期 tick，避免单项目慢采集阻塞整站历史落库。
- 2026-07-26: 101 诊断确认 Docker metadata 路径卡顿时，普通命令超时会残留子进程，且实时与历史链路各自创建 Engine client，导致资源采集持续叠加压力。
- 2026-07-26: 锁定修复为 runner 超时终止子进程，以及实时/历史共享单一 Engine client 的固定 4 请求限流与熔断；不增加配置、API 或 UI。
- 2026-07-26: 审查发现半开探测取消会遗留半开状态；保护令牌的 drop 路径改为恢复带退避的打开状态，并由回归测试锁定。
- 2026-07-26: 审查发现已排队的请求若只等待并发令牌，会在熔断打开后继续被旧请求阻塞；限流等待改为监听熔断打开转换并立即降级，避免在 daemon 退化时延迟采样 worker。

## Key Reasons / Replacements

- 健康状态下继续保持 per-project worker 的独立 cadence；仅在 Docker 控制面已经退化时，允许共享保护器降级跨项目样本，以避免监控本身扩大故障。
- 4xx 容器生命周期竞争不代表 daemon 不可用，保留既有部分成功与局部失败语义；只将连接错误、超时、5xx 和无效成功响应计入熔断。

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
