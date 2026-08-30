# Use Effective Compose Configuration for Service Observation

Dockrev observes a Stack's declared service image from the effective Compose configuration rendered by that Stack's configured Compose CLI. Source Compose configuration remains the auditable input for controlled mutations; it is not an image-observation source when interpolation or multi-file resolution is present.

## Considered Options

- Read raw YAML directly: rejected because valid Compose interpolation is then persisted as an invalid image reference.
- Implement Compose interpolation in Dockrev: rejected because the supported semantics include environment precedence, env files, and multi-file merge behavior already owned by the configured Compose CLI.
- Render with the configured Compose CLI: selected because it observes the same service image declaration that Compose uses to run the Stack.

## Consequences

- Discovery runs one bounded, read-only Compose config render for each project and extracts only the service metadata needed for observation.
- Rendered output is transient and must not be logged or persisted, because it can contain resolved environment values.
- A render failure marks the project `compose_config_unresolved`, preserves the latest accepted Service state, and retries on a later scan. Discovery must not fall back to raw YAML.
- Normal updates consume the effective image reference and continue to use managed overrides. Directly editing an interpolated source `image:` field remains unsupported until variable ownership and mutation semantics are explicitly modeled.
