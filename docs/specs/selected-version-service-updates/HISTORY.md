# 服务版本列表指定版本更新主题历史

> 本文件记录主题边界、兼容性和必要背景；单次任务进度不放在这里。

## Lifecycle / Compatibility

- 指定版本更新使用独立的服务级接口，不放宽通用 `/api/updates` 对当前 candidate 的约束。
- 手动选择较旧但仍高于当前部署版本的版本不会暂停服务的自动更新策略。

## Replacements / Background

- 服务详情页版本列表的呈现与导航由 `ey4ar-service-detail-subpages` 拥有；指定版本的服务端归属与部署语义由本主题拥有。

## Related Changes

- None

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
