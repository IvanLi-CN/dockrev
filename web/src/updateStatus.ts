import type { IconifyIcon } from '@iconify/types'
import alertCircleOutline from '@iconify-icons/mdi/alert-circle-outline'
import arrowUpBoldCircle from '@iconify-icons/mdi/arrow-up-bold-circle'
import closeCircleOutline from '@iconify-icons/mdi/close-circle-outline'
import helpCircleOutline from '@iconify-icons/mdi/help-circle-outline'
import minusCircleOutline from '@iconify-icons/mdi/minus-circle-outline'
import type { Service } from './api'

export type RowStatus = 'ok' | 'updatable' | 'hint' | 'archMismatch' | 'blocked'

type TagSeries = {
  major: number
  minor: number | null
  precision: 1 | 2 | 3
}

const STRICT_SEMVER_PATTERN =
  /^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/

type StrictSemver = {
  major: string
  minor: string
  patch: string
  prerelease: string[]
}

const ORDERED_PRERELEASE_LABELS = new Set(['alpha', 'beta', 'rc', 'pre', 'preview'])

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

function parseStrictSemver(tag: string | null | undefined): StrictSemver | null {
  const trimmed = (tag ?? '').trim()
  if (!trimmed) return null
  const match = STRICT_SEMVER_PATTERN.exec(trimmed)
  if (!match) return null

  const prerelease = (match[4] ?? '')
    .split('.')
    .filter(Boolean)
  if (prerelease.some((token) => /^\d+$/.test(token) && token.length > 1 && token.startsWith('0'))) {
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

function isComparablePrereleaseToken(token: string): boolean {
  if (/^\d+$/.test(token)) return true
  const normalized = token.toLowerCase()
  if (ORDERED_PRERELEASE_LABELS.has(normalized)) return true
  return Array.from(ORDERED_PRERELEASE_LABELS).some((label) => {
    if (!normalized.startsWith(label)) return false
    const suffix = normalized.slice(label.length)
    if (!suffix) return false
    return /^\d+$/.test(suffix) || /^-\d+$/.test(suffix)
  })
}

function isComparableStrictSemver(version: StrictSemver): boolean {
  return version.prerelease.every(isComparablePrereleaseToken)
}

function parseComparableStrictSemver(tag: string | null | undefined): StrictSemver | null {
  const parsed = parseStrictSemver(tag)
  if (!parsed || !isComparableStrictSemver(parsed)) return null
  return parsed
}

function comparableSemverBaseline(
  preferredTag: string | null | undefined,
  fallbackTag: string | null | undefined,
): StrictSemver | null {
  if ((preferredTag ?? '').trim()) return parseComparableStrictSemver(preferredTag)
  return parseComparableStrictSemver(fallbackTag)
}

function semverBaselineForCurrent(svc: Service): StrictSemver | null {
  return comparableSemverBaseline(svc.image.resolvedTag, svc.image.tag)
}

function semverBaselineForCandidate(svc: Service): StrictSemver | null {
  const c = svc.candidate
  if (!c) return null
  return comparableSemverBaseline(c.resolvedTag, c.tag)
}

export function isSemverDowngradeAnomaly(svc: Service): boolean {
  if (!svc.candidate) return false
  const current = semverBaselineForCurrent(svc)
  const candidate = semverBaselineForCandidate(svc)
  if (!current || !candidate) return false
  return compareStrictSemver(candidate, current) < 0
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

  // Candidate always targets the same raw tag; only arch ambiguity should require confirmation.
  if (svc.candidate.archMatch === 'unknown') return 'hint'
  return 'updatable'
}

export function statusDotClass(st: RowStatus): string {
  if (st === 'updatable') return 'statusCircleIcon statusCircleIconPrimary'
  if (st === 'hint') return 'statusCircleIcon statusCircleIconWarn'
  if (st === 'archMismatch') return 'statusCircleIcon statusCircleIconBad'
  if (st === 'blocked') return 'statusCircleIcon statusCircleIconBad'
  return 'statusCircleIcon statusCircleIconNeutral'
}

export function statusIcon(st: RowStatus): IconifyIcon {
  if (st === 'updatable') return arrowUpBoldCircle
  if (st === 'hint') return helpCircleOutline
  if (st === 'archMismatch') return alertCircleOutline
  if (st === 'blocked') return closeCircleOutline
  return minusCircleOutline
}

export function statusLabel(st: RowStatus): string {
  if (st === 'updatable') return '可更新'
  if (st === 'hint') return '需确认'
  if (st === 'archMismatch') return '架构不匹配'
  if (st === 'blocked') return '被阻止'
  return '无更新'
}

export function blockedReasonFor(svc: Service): string | null {
  if (svc.ignore?.matched) return svc.ignore.reason ?? '被阻止'
  return null
}

export function noteFor(svc: Service, st: RowStatus): string {
  if (st === 'blocked') return blockedReasonFor(svc) ?? '被阻止'
  if (st === 'archMismatch') return '仅提示，不允许更新'
  if (st === 'hint') {
    if (isSemverDowngradeAnomaly(svc)) return '⚠ 版本异常：候选版本低于当前版本'
    if (svc.candidate?.archMatch === 'unknown') return 'arch 未知'
    return ''
  }
  if (st === 'updatable') {
    const hasForceBackup =
      Object.values(svc.settings.backupTargets.bindPaths).some((v) => v === 'force') ||
      Object.values(svc.settings.backupTargets.volumeNames).some((v) => v === 'force')
    const semverAnomaly = isSemverDowngradeAnomaly(svc)
    if (semverAnomaly && hasForceBackup) return '⚠ 版本异常：候选版本低于当前版本；备份通过后执行'
    if (semverAnomaly) return '⚠ 版本异常：候选版本低于当前版本'
    // The "按当前标签序列" hint became low-value after version popovers were introduced; keep notes
    // only when there's an operator-relevant extra step.
    return hasForceBackup ? '备份通过后执行' : ''
  }
  return ''
}
