const STRICT_SEMVER_PATTERN = /^v?\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/

function trimOrEmpty(value: string | null | undefined): string {
  return (value ?? '').trim()
}

export function isStrictSemverTag(tag: string | null | undefined): boolean {
  const t = trimOrEmpty(tag)
  return t.length > 0 && STRICT_SEMVER_PATTERN.test(t)
}

function isPending(status: string | null | undefined): boolean {
  return trimOrEmpty(status) === 'pending'
}

export function formatCurrentTagDisplay(
  tag: string,
  resolvedTag: string | null | undefined,
  inferenceStatus?: string | null,
): string {
  if (isPending(inferenceStatus)) return '等待中…'
  const resolved = trimOrEmpty(resolvedTag)
  if (isStrictSemverTag(resolved)) return resolved

  const raw = trimOrEmpty(tag)
  if (!raw) return '-'
  if (isStrictSemverTag(raw)) return raw
  return '-'
}

export function formatCandidateTagDisplay(
  tag: string,
  resolvedTag: string | null | undefined,
  _inferenceStatus?: string | null,
): string {
  void _inferenceStatus
  const resolved = trimOrEmpty(resolvedTag)
  if (isStrictSemverTag(resolved)) return resolved

  const raw = trimOrEmpty(tag)
  if (!raw) return '-'
  if (isStrictSemverTag(raw)) return raw
  return raw
}
