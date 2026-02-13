export function normalizeDigest(digest: string | null | undefined): string | null {
  const raw = (digest ?? '').trim()
  if (!raw) return null
  return raw.includes(':') ? raw : `sha256:${raw}`
}

export function shortenDigest(digest: string, keep: number = 12): string {
  const normalized = normalizeDigest(digest) ?? digest
  const parts = normalized.split(':')
  if (parts.length < 2) return normalized
  const prefix = parts[0]
  const rest = parts.slice(1).join(':')
  if (rest.length <= keep) return normalized
  return `${prefix}:${rest.slice(0, keep)}…`
}

