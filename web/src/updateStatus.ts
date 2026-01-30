import type { Service } from './api'

export type RowStatus = 'ok' | 'updatable' | 'hint' | 'crossTag' | 'archMismatch' | 'blocked'

type TagSeries = {
  major: number
  minor: number | null
  precision: 1 | 2 | 3
}

function parseTagSeries(tag: string): TagSeries | null {
  let t = tag.trim()
  if (!t) return null
  if (t.startsWith('v')) t = t.slice(1)
  if (!t) return null

  // Best-effort: accept semver core with optional prerelease/build.
  const core = t.split(/[+-]/, 1)[0]
  const parts = core.split('.')
  if (parts.length < 1 || parts.length > 3) return null
  if (!parts.every((p) => /^\d+$/.test(p))) return null

  const nums = parts.map((p) => Number(p))
  if (!nums.every((n) => Number.isFinite(n) && n >= 0)) return null

  return {
    major: nums[0],
    minor: parts.length >= 2 ? nums[1] : null,
    precision: parts.length as 1 | 2 | 3,
  }
}

export function tagSeriesMatches(currentTag: string, candidateTag: string): boolean | null {
  const cur = parseTagSeries(currentTag)
  const cand = parseTagSeries(candidateTag)
  if (!cur || !cand) return null
  if (cur.major !== cand.major) return false
  if (cur.precision === 1) return true
  return cur.minor === cand.minor
}

export function serviceRowStatus(svc: Service): RowStatus {
  if (svc.ignore?.matched) return 'blocked'
  if (!svc.candidate) return 'ok'
  if (svc.candidate.archMatch === 'mismatch') return 'archMismatch'

  const effectiveCurrentTag = svc.image.resolvedTag ?? svc.image.tag
  const seriesMatch = tagSeriesMatches(effectiveCurrentTag, svc.candidate.tag)
  if (seriesMatch === false) return 'crossTag'

  // "unknown" arch and/or unparseable tags are still actionable, but should be treated as "needs confirmation".
  if (svc.candidate.archMatch === 'unknown' || seriesMatch == null) return 'hint'
  return 'updatable'
}

export function statusDotClass(st: RowStatus): string {
  if (st === 'updatable') return 'statusDot statusDotOk'
  if (st === 'hint') return 'statusDot statusDotWarn'
  if (st === 'crossTag') return 'statusDot statusDotWarn'
  if (st === 'archMismatch') return 'statusDot statusDotBad'
  if (st === 'blocked') return 'statusDot statusDotBad'
  return 'statusDot'
}

export function statusLabel(st: RowStatus): string {
  if (st === 'updatable') return '可更新'
  if (st === 'hint') return '需确认'
  if (st === 'crossTag') return '跨标签版本'
  if (st === 'archMismatch') return '架构不匹配'
  if (st === 'blocked') return '被阻止'
  return '无更新'
}

export function noteFor(svc: Service, st: RowStatus): string {
  if (st === 'blocked') return svc.ignore?.reason ?? '被阻止'
  if (st === 'archMismatch') return '仅提示，不允许更新'
  if (st === 'crossTag') return '候选标签不匹配当前序列'
  if (st === 'hint') {
    if (svc.candidate?.archMatch === 'unknown') return 'arch 未知'
    return '标签关系不确定'
  }
  if (st === 'updatable') {
    const hasForceBackup =
      Object.values(svc.settings.backupTargets.bindPaths).some((v) => v === 'force') ||
      Object.values(svc.settings.backupTargets.volumeNames).some((v) => v === 'force')
    return hasForceBackup ? '备份通过后执行' : '按当前标签序列'
  }
  return '-'
}
