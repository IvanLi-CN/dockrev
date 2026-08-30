# Dockrev 品牌媒体主题与比例实现状态

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖与 rollout 相关事实。

## Current Status

- Implementation: 暗色 4:5 海报已实现；亮色 4:5 海报进行中，两个社交图待实现。
- Lifecycle: active
- Catalog note: 四资产合同已建立，暗色海报作为首个切片已交付。

## Coverage / rollout summary

- 品牌生成脚本将收敛为按原尺寸复制已校验的主题媒体资产，避免将海报拉伸为旧的 2:3 交付尺寸。
- 旧的无主题后缀海报将继续作为暗色交付物的兼容副本。

## Remaining Gaps

- 完成亮色 4:5 海报的生成、视觉确认与接入。
- 生成并接入亮色和暗色 2:1 社交图。
- 在明确默认主题后迁移无主题后缀的社交图消费者。

## Related Changes

- `docs/branding/generate_brand_assets.py`
- `docs/branding/generated/dockrev-product-poster-dark-imagegen-candidate.png`

## References

- `./SPEC.md`
- `./HISTORY.md`
