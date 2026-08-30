# Job Detail Error Illustration Visual Review

## Review Object

The Job Detail initial-load error state renders the Apache-2.0 `Error` illustration from Adobe Spectrum's `@spectrum-icons/illustrations` package. The component keeps the package's original SVG geometry inline so Dockrev theme tokens can control the two stroke colours without shipping separate light and dark drawings.

- `web/src/components/jobDetailErrorIllustration/JobDetailErrorIllustrationAsset.tsx`
- Source package: `@spectrum-icons/illustrations@3.7.1`
- Source component: `Error` / `Error500`
- License notice: `web/src/components/jobDetailErrorIllustration/assets/SPECTRUM-APACHE-2.0-LICENSE.txt`

The source artwork is a transparent 146.569 by 94 SVG showing three server rows and a warning triangle. It is copied from the package's published `Error.js` path and circle primitives; no generated, traced, raster or alternative geometry is used.

## Non-negotiable Asset Contract

| Check | Requirement | Acceptance method |
| --- | --- | --- |
| Source form | One official Adobe Spectrum SVG geometry, with `viewBox="0 0 146.569 94"`; no hand-drawn replacement. | Inspect package provenance, local component and license notice. |
| Geometry parity | Light and dark themes use the same inline path, circles, dimensions and transforms. | Review one component source; only CSS variables resolve differently. |
| Theme control | `--job-detail-error-illustration-primary` and `--job-detail-error-illustration-error` are the only colour controls. | Toggle Dockrev themes in the mock-only route and inspect computed SVG styles. |
| Transparency | No background paint is present; the page surface remains visible around the server rows and warning mark. | Reject root background primitives and render on both page themes. |
| Vector integrity | No `<image>`, `foreignObject`, filter, mask, clip path or raster payload. | `web/tests/jobDetailErrorIllustrationAsset.test.tsx`. |
| License | The Apache-2.0 notice ships beside the source component. | Check the included notice file. |

## Layout Contract

| Relationship | Desktop | Mobile |
| --- | ---: | ---: |
| Illustration width | `216px` maximum | `min(212px, 64vw)` |
| Illustration to recovery group | `32px` | `28px` |
| Copy to retry action | `18px` | `16px` |
| Optical correction | `translateX(-6px)` | `translateX(-5px)` |
| Retry target | intrinsic `36px` high | `312px` wide and `44px` high |

The illustration's layout box remains centered on the error region's main axis. The small negative translation corrects the artwork's right-heavy server-line mass; it does not move the copy, button or focus order.

## Visual Evaluation Requirements

| Area | Required outcome |
| --- | --- |
| Meaning | Three server rows plus the rose warning triangle communicate that task details could not be read and that recovery is available. |
| Light theme | Server strokes are crisp Dockrev blue, the warning mark is rose, and no black, gray wash or dirty background appears. |
| Dark theme | Cyan server strokes and rose warning remain crisp against the navy page without white halo, blur or faded duplicate artwork. |
| Composition | The illustration, error copy and retry action form a centered column with two deliberate spacing levels and no redundant error-coloured frame. |
| Desktop | At `1440 x 900` CSS px, the illustration is bounded, optically centered and subordinate to the page title. |
| Mobile | At `393 x 852` CSS px, the illustration fits without horizontal scrolling, keeps its aspect ratio, and leaves a reachable full-width retry action. |
| Recovery | The first-load error replaces the skeleton; a successful retry removes the illustration and mounts the Job Detail data region. |

## Review Procedure

1. Verify source provenance, Apache notice, one-source geometry, transparency and vector-only constraints through `web/tests/jobDetailErrorIllustrationAsset.test.tsx`.
2. Render the mock-only `job-detail-retry` failure and recovery states in light and dark themes at `1440 x 900` and `393 x 852` CSS px. Check each requirement above and confirm no horizontal overflow.
3. Activate retry in the same demo and confirm Job Detail replaces the error state.
4. Normalize screenshots using the page `trim_only` policy and compare each candidate at its final Spec asset path before persisting evidence. Store the reviewed images after owner confirmation.
