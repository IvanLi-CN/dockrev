export function imageRepoFromImageRef(imageRef: string | null | undefined): string | null {
  const raw = (imageRef ?? '').trim()
  if (!raw) return null

  // Align with backend `registry::ImageRef::parse`:
  // - strip optional @digest
  // - split `repo/name:tag` from the right
  // - registry is the first segment if it contains '.' or ':', otherwise docker.io
  // - docker.io names without a slash are normalized to `library/<name>`
  const withoutDigest = raw.includes('@') ? raw.split('@', 1)[0] : raw
  const lastColon = withoutDigest.lastIndexOf(':')
  if (lastColon < 0) return null

  const nameWithRegistry = withoutDigest.slice(0, lastColon).trim()
  const tag = withoutDigest.slice(lastColon + 1).trim()
  if (!nameWithRegistry || !tag || tag.includes('/')) return null

  const parts = nameWithRegistry.split('/').filter(Boolean)
  if (parts.length === 0) return null

  let registry = 'docker.io'
  let name = ''
  if (parts[0].includes('.') || parts[0].includes(':')) {
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
