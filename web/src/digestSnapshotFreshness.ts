const SNAPSHOT_CLOCK_SKEW_TOLERANCE_MS = 60_000

export type SnapshotFreshnessBaseline = {
  checkedAt: string | null
  startedAtMs: number
}

function trimOrNull(value: string | null | undefined): string | null {
  const trimmed = (value ?? '').trim()
  return trimmed || null
}

function parseTimestampMs(value: string | null | undefined): number | null {
  const trimmed = trimOrNull(value)
  if (!trimmed) return null
  const ms = Date.parse(trimmed)
  return Number.isFinite(ms) ? ms : null
}

export function createSnapshotFreshnessBaseline(
  checkedAt: string | null | undefined,
): SnapshotFreshnessBaseline {
  return {
    checkedAt: trimOrNull(checkedAt),
    startedAtMs: Date.now(),
  }
}

export function isSnapshotFreshEnough(
  checkedAt: string | null | undefined,
  baseline: SnapshotFreshnessBaseline | null | undefined,
): boolean {
  if (!baseline) return true

  const snapshotMs = parseTimestampMs(checkedAt)
  if (snapshotMs == null) return false

  const baselineMs = parseTimestampMs(baseline.checkedAt)
  if (baselineMs != null) return snapshotMs > baselineMs

  return snapshotMs + SNAPSHOT_CLOCK_SKEW_TOLERANCE_MS >= baseline.startedAtMs
}
