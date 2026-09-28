# Dockrev Manual Version Release Delivery History

## Lifecycle / Compatibility

- This active contract replaces PR-label-based release decisions with one
  required manual `version` value.
- ADR 0015 supersedes ADRs 0008, 0009, 0010, 0011, and 0012 for current
  release allocation, channel, identity, and recovery policy.
- Historical Specs and ADRs remain available as historical context; they do
  not authorize label allocation or identity backfill.

## Replacements / Background

- The main root `VERSION` file is now both the identity written by preparation
  and the sole numeric baseline for the next decision.
- Stable, alpha, beta, and RC delivery retains immutable reservations,
  identity-bound publication, complete asset coverage, failure notification,
  and same-identity recovery.
- Dockrev's Style Playbook project snapshot is updated separately. The generic
  Manual Version Release Delivery topic is not modified by this project
  implementation.

## References

- [SPEC.md](./SPEC.md)
- [IMPLEMENTATION.md](./IMPLEMENTATION.md)
