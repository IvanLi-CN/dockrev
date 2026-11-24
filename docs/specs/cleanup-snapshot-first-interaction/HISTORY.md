# Cleanup 快照优先与即时确认弹窗 主题历史

> 这里记录主题局部生命周期、替换、兼容性与必要背景；完整 ADR 取舍保留在 `docs/adr/`。

## Lifecycle / Compatibility

- 2026-11-25：创建主题，冻结快照优先加载、即时确认弹窗、前台自动刷新与 apply 准入合同。
- 服务端响应新增 `snapshotAgeMs`，不删除或重命名既有字段。
- `POST /api/cleanups/scan-runs` 继续只接受 `reason=page`、`preset=aggressive`、`scope=all`；弹窗发现态复用该流并做本地投影。

## Replacements / Background

- 本主题取代 `qynjg-docker-prune-cleanup-console` 中以下交互规则：
  - 页面首次进入必须强制触发 `reason=page` 扫描；
  - 任一动作必须先完成 `reason=confirm` scoped scan 才能打开弹窗；
  - confirm 首次请求必须 `refresh=true` 并轮询到 ready；
  - 任何扫描运行期间 apply 必须等待。
- 保留的规则：preset 投影、scope 归属、fingerprint 语义、volume 实例校验、apply 的 300 秒新鲜度边界。

## Related Changes

- ADR: [`docs/adr/0023-cleanup-reads-existing-snapshot.md`](../../adr/0023-cleanup-reads-existing-snapshot.md)

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
