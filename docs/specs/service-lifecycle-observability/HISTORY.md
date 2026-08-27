# Dockrev：服务生命周期可观测性 主题历史

> 这里记录主题局部生命周期、替换、兼容性与必要背景；完整 ADR 取舍保留在 `docs/adr/`。

## Lifecycle / Compatibility

- 新增主题；事件账本与既有资源监控、Docker 日志和 Job 日志并存，旧接口字段保持兼容。

## Replacements / Background

- 资源采样缺口和 Job 文本继续作为各自领域的数据，不再承担生命周期事实来源。
- 相关架构取舍见 `docs/adr/0001-service-lifecycle-event-ledger.md`。

## Current lifecycle

- 建立 `service_lifecycle_events` 30 天账本、Engine events 观测器、生命周期 REST/SSE、资源 history 标记与服务日志 union。
- 保留 Compose 原有 start/stop/restart 语义；缺少 Engine events 权限时继续执行并持久记录不完整观察。

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
