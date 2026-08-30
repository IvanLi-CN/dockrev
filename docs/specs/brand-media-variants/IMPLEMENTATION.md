# Dockrev 品牌媒体主题与比例实现状态

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖与 rollout 相关事实。

## Current Status

- Implementation: 暗色与亮色 4:5 海报已实现；两个 2:1 社交图待实现。
- Lifecycle: active
- Catalog note: 四资产合同已建立；两个海报变体已交付。

## Coverage / rollout summary

- 品牌生成脚本从暗色 4:5 母版逐像素生成亮色变体，再按原尺寸复制已校验的主题媒体资产，避免拉伸为旧的 2:3 交付尺寸。
- 旧的无主题后缀海报将继续作为暗色交付物的兼容副本。

## Remaining Gaps

- 生成并接入亮色和暗色 2:1 社交图。
- 在明确默认主题后迁移无主题后缀的社交图消费者。

## Related Changes

- `docs/branding/generate_brand_assets.py`
- `docs/branding/recolor_product_poster.py`
- `docs/branding/generated/dockrev-product-poster-dark-imagegen-candidate.png`
- `docs/branding/generated/dockrev-product-poster-light-imagegen-candidate.png`（仅保留生成溯源，不作为交付源）

## References

- `./SPEC.md`
- `./HISTORY.md`

## Visual Evidence

- `./assets/dockrev-product-poster-dark.png`
- `./assets/dockrev-product-poster-light.png`
