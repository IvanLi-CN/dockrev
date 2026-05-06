# Implementation

## 当前覆盖

- Backend:
  - `service_tag_history` schema, upsert, and suggestion listing.
  - `GET /api/services/{service_id}/tag-suggestions`.
  - `PUT /api/services/{service_id}/compose-tag`.
  - Compose image tag patching that preserves line indentation, quote style, and trailing comments without whole-file YAML reserialization, including implicit `latest` refs such as `image: nginx`.
  - Successful update jobs now record effective target tags as future suggestions.
- Frontend:
  - Service settings drawer `部署 tag` editor with current tag display, lazy suggestions, keyboard-selectable options, loading/empty/error states, and field-level save errors.
  - API client/types and Storybook mock handlers for suggestions and compose-tag saves.
  - Storybook coverage for desktop suggestions, save error, and mobile drawer state.

## 验证

- `cargo fmt --all --check`
- `cargo test -p dockrev-api`
- `cd web && bun run lint`
- `cd web && bun run build`
- `cd web && bun run build-storybook`
- `cd web && bun run test-storybook`
