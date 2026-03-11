export function imageRepoFromImageRef(imageRef: string | null | undefined): string | null {
  const raw = (imageRef ?? '').trim()
  if (!raw) return null

  // Parse image references into `registry/name` (aka image repo).
  // Supported forms:
  // - `repo/name:tag`
  // - `repo/name:tag@sha256:...`
  // - `repo/name@sha256:...` (digest-only)
  // - registry is the first segment if it contains '.', ':', or is exactly 'localhost'
  // - docker.io names without a slash are normalized to `library/<name>`
  const at = raw.indexOf('@')
  const hasDigest = at >= 0
  const withoutDigest = (hasDigest ? raw.slice(0, at) : raw).trim()
  if (!withoutDigest) return null

  const lastSlash = withoutDigest.lastIndexOf('/')
  const lastColon = withoutDigest.lastIndexOf(':')
  const hasTag = lastColon > lastSlash

  const nameWithRegistry = (hasTag ? withoutDigest.slice(0, lastColon) : withoutDigest).trim()
  const tag = hasTag ? withoutDigest.slice(lastColon + 1).trim() : ''
  if (!nameWithRegistry) return null
  if (hasTag) {
    if (!tag || tag.includes('/')) return null
  } else if (!hasDigest) {
    // For tagless refs, only accept digest-pinned form; keep behavior aligned with the
    // backend parser for plain `repo/name` inputs.
    return null
  }

  const parts = nameWithRegistry.split('/').filter(Boolean)
  if (parts.length === 0) return null

  let registry = 'docker.io'
  let name = ''
  if (parts[0].includes('.') || parts[0].includes(':') || parts[0] === 'localhost') {
    registry = parts[0].trim() || registry
    name = parts.slice(1).join('/')
  } else {
    name = nameWithRegistry
  }

  name = name.trim()
  if (!name) return null
  if (registry === 'docker.io' && !name.includes('/')) name = `library/${name}`

  return `${registry}/${name}`.toLowerCase()
}
