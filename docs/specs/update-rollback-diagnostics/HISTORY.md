# Dockrev：自动更新回滚诊断主题历史

> 这里记录主题局部生命周期、兼容性与必要背景；完整 ADR 取舍保留在 `docs/adr/`。规范正文仍以 `./SPEC.md` 为准。

## Lifecycle / Compatibility

- The topic is active and has no implementation yet.
- Existing jobs have no evidence BLOB and remain readable. Evidence is optional for every job type and is created only for health-triggered update rollback handling.
- Existing terminal-job retention remains the evidence retention policy.

## Replacements / Background

- The fixed 90-second health wait observed only the health status and destroyed candidate evidence during automatic rollback.
- The durable contract replaces that observability gap with candidate-effective policy waiting, private per-job spool files, and one per-job `tar.zst` archive.
- The archive layout preserves service boundaries while keeping the confirmed single BLOB storage boundary.

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
- [Store Rollback Evidence with Its Update Job](../../adr/0002-update-rollback-evidence-storage.md)
