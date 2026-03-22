# DB Contract

## `services`

- Add nullable column: `repo_url TEXT`
- Existing rows default to `NULL`
- Value semantics:
  - `NULL`: no persisted repository link
  - non-null: absolute `http(s)` repository URL
