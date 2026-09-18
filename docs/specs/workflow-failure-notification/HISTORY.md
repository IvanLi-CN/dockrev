# Dockrev：预期成功工作流失败通知主题历史

## Lifecycle / Compatibility

- Existing `Notify failed release` remains the Release-specific notification boundary and keeps its release identity validation, artifact context, and same-SHA recovery semantics.
- `Notify failed workflow` adds the generic path for other expected-success workflows; it does not replace or weaken the Release path.
- The checked-in transport selection remains `oidrune-oidc` with no repository secret; external OIDC and gateway configuration stays outside this repository.

## Replacements / Background

- The previous implicit rule “only Release failures are notified” is replaced by an explicit expected-success workflow allowlist.
- Ordinary workflow failures are no longer treated as release failures. They use run metadata and a rerun/recovery reminder, while Release continues to use identity-aware context.
- Notification workflows are explicitly excluded so a notifier failure does not recursively produce another notification.

## References

- [SPEC.md](./SPEC.md)
- [IMPLEMENTATION.md](./IMPLEMENTATION.md)
- Style Playbook Topic: `Release failure notification`
