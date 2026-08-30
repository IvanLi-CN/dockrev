# Dockrev 品牌媒体主题与比例主题历史

> 这里记录主题局部生命周期、替换、兼容性与必要背景；完整 ADR 取舍保留在 `docs/adr/`。规范正文仍以 `./SPEC.md` 为准。

## Lifecycle / Compatibility

- 主题媒体采用显式亮/暗文件名；无主题后缀的海报在消费者迁移期间保留为暗色兼容副本。

## Replacements / Background

- 旧海报为暗色竖版视觉，但公开交付尺寸为 1024x1472，未满足 4:5 合同。
- 新暗色海报以旧海报为视觉基线，只重排布局以纳入 4:5 画布。

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
