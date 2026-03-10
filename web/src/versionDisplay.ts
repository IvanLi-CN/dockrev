const STRICT_SEMVER_PATTERN =
  /^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/

type StrictSemver = {
  major: string
  minor: string
  patch: string
  prerelease: string[]
}

function trimOrEmpty(value: string | null | undefined): string {
  return (value ?? '').trim()
}

function parseStrictSemver(tag: string): StrictSemver | null {
  const trimmed = tag.trim()
  if (!trimmed) return null
  const match = STRICT_SEMVER_PATTERN.exec(trimmed)
  if (!match) return null

  const prerelease = (match[4] ?? '')
    .split('.')
    .filter(Boolean)

  // Semver numeric identifiers must not include leading zeros.
  if (
    prerelease.some(
      (token) => /^\d+$/.test(token) && token.length > 1 && token.startsWith('0'),
    )
  ) {
    return null
  }

  return {
    major: match[1],
    minor: match[2],
    patch: match[3],
    prerelease,
  }
}

function compareNumericToken(a: string, b: string): number {
  const aNum = /^\d+$/.test(a)
  const bNum = /^\d+$/.test(b)
  if (aNum && bNum) {
    // Compare numeric tokens without parsing as Number to avoid overflow.
    if (a.length !== b.length) return a.length - b.length
    if (a === b) return 0
    return a < b ? -1 : 1
  }
  if (aNum) return -1
  if (bNum) return 1
  if (a === b) return 0
  return a < b ? -1 : 1
}

function compareStrictSemver(a: StrictSemver, b: StrictSemver): number {
  const majorCmp = compareNumericToken(a.major, b.major)
  if (majorCmp !== 0) return majorCmp
  const minorCmp = compareNumericToken(a.minor, b.minor)
  if (minorCmp !== 0) return minorCmp
  const patchCmp = compareNumericToken(a.patch, b.patch)
  if (patchCmp !== 0) return patchCmp

  const aPre = a.prerelease
  const bPre = b.prerelease
  if (aPre.length === 0 && bPre.length === 0) return 0
  if (aPre.length === 0) return 1
  if (bPre.length === 0) return -1

  const len = Math.max(aPre.length, bPre.length)
  for (let i = 0; i < len; i += 1) {
    const aTok = aPre[i]
    const bTok = bPre[i]
    if (aTok == null) return -1
    if (bTok == null) return 1
    const cmp = compareNumericToken(aTok, bTok)
    if (cmp !== 0) return cmp
  }
  return 0
}

function parseBackendVersion(tag: string): StrictSemver | null {
  const trimmed = tag.trim()
  const withoutV = trimmed.startsWith('v') ? trimmed.slice(1) : trimmed
  if (!withoutV) return null

  // Backend rule: try strict semver parse first.
  const direct = parseStrictSemver(withoutV)
  if (direct) return direct

  // Backend rule: extract numeric prefix (digits + dots), then parse/coerce to semver.
  let prefix = ''
  for (const ch of withoutV) {
    if ((ch >= '0' && ch <= '9') || ch === '.') {
      prefix += ch
      continue
    }
    break
  }
  prefix = prefix.replace(/\.+$/, '')
  if (!prefix) return null

  const prefixSemver = parseStrictSemver(prefix)
  if (prefixSemver) return prefixSemver

  const parts = prefix.split('.')
  const coerced =
    parts.length === 1
      ? `${parts[0]}.0.0`
      : parts.length === 2
        ? `${parts[0]}.${parts[1]}.0`
        : null
  if (!coerced) return null

  return parseStrictSemver(coerced)
}

function inferSemverTagsFromSnapshot(
  tags: Array<string | null | undefined> | null | undefined,
  rawTag: string | null | undefined,
): string[] {
  const rawTrim = trimOrEmpty(rawTag)
  const items: Array<{ version: StrictSemver; tag: string }> = []

  for (const tag of tags ?? []) {
    const t = trimOrEmpty(tag)
    if (!t) continue
    const version = parseBackendVersion(t)
    if (!version) continue
    items.push({ version, tag: t })
  }

  items.sort((a, b) => {
    const vCmp = compareStrictSemver(b.version, a.version) // desc
    if (vCmp !== 0) return vCmp
    if (a.tag === b.tag) return 0
    return a.tag < b.tag ? 1 : -1 // desc
  })

  return items
    .map((item) => item.tag)
    .filter((tag) => (rawTrim ? tag !== rawTrim : true))
}

export function isStrictSemverTag(tag: string | null | undefined): boolean {
  const t = trimOrEmpty(tag)
  return t.length > 0 && parseStrictSemver(t) != null
}

export function pickSnapshotDisplayTag(
  tags: Array<string | null | undefined> | null | undefined,
  rawTag: string | null | undefined,
): string | null {
  const rawTrim = trimOrEmpty(rawTag)
  const rawStrict = rawTrim ? parseStrictSemver(rawTrim) : null

  // Align with backend inference: only derive a local display tag when the raw tag itself is
  // not already strict semver (including prerelease). Otherwise we risk rewriting the
  // deployment semantics (e.g. `v1.2.3-rc.1` -> `v1.2.3`) just because the digest also has
  // another tag pointing at it.
  if (rawStrict) return null

  const inferred = inferSemverTagsFromSnapshot(tags, rawTrim)
  return inferred[0] ?? null
}

function isPending(status: string | null | undefined): boolean {
  return trimOrEmpty(status) === 'pending'
}

export function formatCurrentTagDisplay(
  tag: string,
  resolvedTag: string | null | undefined,
  inferenceStatus?: string | null,
): string {
  if (isPending(inferenceStatus)) return '加载中…'
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
