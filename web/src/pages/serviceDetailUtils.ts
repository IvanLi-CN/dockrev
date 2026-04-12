import { useEffect, useRef } from 'react'
import type {
  Service,
  ServiceDigestTagsScanSummary,
  ServiceRollbackTargetResponse,
} from '../api'
import { normalizeDigest } from '../components/digest'
import { isDockrevImageRef } from '../runtimeConfig'
import { serviceRowStatus } from '../updateStatus'
import { isStrictSemverTag } from '../versionDisplay'

export function errorMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e)
}

export function scanHasFailures(scan: ServiceDigestTagsScanSummary | null | undefined): boolean {
  if (!scan) return false
  return scan.manifestsTimeout > 0 || scan.manifestsError > 0
}

export function scanIsComplete(scan: ServiceDigestTagsScanSummary | null | undefined): boolean {
  if (!scan) return false
  return scan.repoTagsConsidered >= scan.repoTagsTotal
}

export function shortDigest(digest: string) {
  return digest.length <= 24 ? digest : `${digest.slice(0, 12)}…${digest.slice(-8)}`
}

export function isDockrevService(svc: Service): boolean {
  return isDockrevImageRef(svc.image.ref)
}

export function svcTone(svc: Service): 'ok' | 'warn' | 'bad' | 'muted' {
  const st = serviceRowStatus(svc)
  if (st === 'updatable') return 'ok'
  if (st === 'hint') return 'warn'
  if (st === 'archMismatch' || st === 'blocked') return 'bad'
  return 'muted'
}

export function formatMap(map: Record<string, string>) {
  const keys = Object.keys(map)
  if (keys.length === 0) return []
  return keys.map((key) => ({ key, value: map[key] }))
}

export function rollbackUnavailableReasonLabel(reason: string | null | undefined): string | undefined {
  switch ((reason ?? '').trim()) {
    case '':
      return undefined
    case 'rollback_in_progress':
      return '回滚任务进行中，点击可查看任务详情'
    case 'service_update_in_progress':
      return '该服务已有更新任务进行中'
    case 'stack_update_in_progress':
      return '该堆栈已有更新任务进行中'
    case 'global_update_in_progress':
      return '当前存在全局更新任务进行中'
    case 'dockrev_service_managed_via_supervisor':
      return 'Dockrev 自身回滚请使用 Supervisor 页面'
    case 'current_digest_missing':
      return '当前运行摘要未知，暂时无法回滚'
    case 'target_digest_matches_current':
      return '升级前版本与当前运行摘要一致，无需回滚'
    case 'no_matching_update_history':
      return '未找到可回滚到升级前版本的成功升级记录'
    default:
      return '当前不可回滚'
  }
}

export function rollbackVersionLabel(displayTag: string | null | undefined, digest: string | null | undefined): string {
  const tag = (displayTag ?? '').trim()
  return tag || ((digest ?? '').trim() ? shortDigest((digest ?? '').trim()) : '-')
}

export const ROLLBACK_TARGET_REFRESH_HINT = '回滚信息刷新中…'

export function normalizeMaybeDigest(value: string | null | undefined): string | null {
  return normalizeDigest(value)?.toLowerCase() ?? null
}

export function rollbackTargetMatchesServiceDigest(
  svc: Service | null,
  target: ServiceRollbackTargetResponse | null,
): boolean {
  if (!svc) return target == null
  if (isDockrevService(svc)) return target == null
  const serviceDigest = normalizeMaybeDigest(svc.image.digest)
  const rollbackCurrentDigest = normalizeMaybeDigest(target?.currentDigest)
  return (serviceDigest ?? '') === (rollbackCurrentDigest ?? '')
}

export function shouldPrefetchFloatingCandidate(
  candidateTag: string | null | undefined,
  candidateResolvedTag: string | null | undefined,
  candidateDigest: string | null | undefined,
): boolean {
  const raw = (candidateTag ?? '').trim()
  return raw !== '-' && Boolean(raw) && !isStrictSemverTag(raw) && !isStrictSemverTag(candidateResolvedTag) && (candidateDigest ?? '').trim().length > 0
}

export function useRollbackTargetInvariantWarning(
  service: Service | null,
  rollbackTarget: ServiceRollbackTargetResponse | null,
) {
  const rollbackInvariantWarnKeyRef = useRef<string | null>(null)

  useEffect(() => {
    if (!service || !rollbackTarget || isDockrevService(service)) {
      rollbackInvariantWarnKeyRef.current = null
      return
    }
    if (rollbackTargetMatchesServiceDigest(service, rollbackTarget)) {
      rollbackInvariantWarnKeyRef.current = null
      return
    }
    const key = [
      service.id,
      normalizeMaybeDigest(service.image.digest) ?? '',
      normalizeMaybeDigest(rollbackTarget.currentDigest) ?? '',
    ].join(':')
    if (rollbackInvariantWarnKeyRef.current === key) return
    rollbackInvariantWarnKeyRef.current = key
    console.warn('[dockrev] rollback target digest invariant violated', {
      serviceId: service.id,
      serviceDigest: normalizeMaybeDigest(service.image.digest),
      rollbackCurrentDigest: normalizeMaybeDigest(rollbackTarget.currentDigest),
      reason: 'state_invariant_mismatch',
    })
  }, [rollbackTarget, service])
}
