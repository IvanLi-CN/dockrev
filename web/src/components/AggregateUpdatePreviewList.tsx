import { useEffect, useState } from 'react'
import { Icon } from '@iconify/react'
import helpCircleOutline from '@iconify-icons/mdi/help-circle-outline'

import type { Service } from '../api'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../ui'
import { isSemverDowngradeAnomaly, type RowStatus } from '../updateStatus'
import { CurrentVersionPopover } from './CurrentVersionPopover'
import { normalizeDigest } from './digest'
import { VersionTagsPopover } from './VersionTagsPopover'
import {
  formatCandidateTagDisplay,
  formatCurrentTagDisplay as formatTagDisplay,
  inferResolvedTagsFromSnapshot,
  isStrictSemverTag,
} from '../versionDisplay'
import { DIGEST_SNAPSHOT_UPDATED_EVENT, type DigestSnapshotUpdatedDetail } from '../digestInferenceTracker'
import { imageRepoFromImageRef } from '../imageRepo'

export type AggregateUpdatePreviewListItem = {
  svc: Service
  status: Extract<RowStatus, 'updatable' | 'hint'>
  guardedDockrev?: boolean
  displayName?: string
  stackId?: string
}

function splitImageRef(ref: string): { registry: string; name: string } {
  const s = ref.trim()
  const withoutDigest = s.includes('@') ? s.split('@', 1)[0] : s
  const firstSlash = withoutDigest.indexOf('/')
  if (firstSlash < 0) {
    return { registry: 'docker.io', name: withoutDigest }
  }
  const firstSeg = withoutDigest.slice(0, firstSlash)
  const rest = withoutDigest.slice(firstSlash + 1)
  const isRegistry = firstSeg.includes('.') || firstSeg.includes(':') || firstSeg === 'localhost'
  if (isRegistry) return { registry: firstSeg, name: rest }
  return { registry: 'docker.io', name: withoutDigest }
}

function splitImageNameForDisplay(name: string, tag: string | null | undefined): { base: string; suffix: string } {
  const n = name.trim() || '-'
  const t = (tag ?? '').trim()
  if (!t) return { base: n, suffix: '' }
  if (t.startsWith('sha256:')) return { base: n, suffix: `@${t}` }
  return { base: n, suffix: `:${t}` }
}

function shouldPrefetchFloatingCandidate(
  candidateTag: string | null | undefined,
  candidateResolvedTag: string | null | undefined,
  candidateDigest: string | null | undefined,
): boolean {
  const raw = (candidateTag ?? '').trim()
  if (raw === '-') return false
  if (!raw || isStrictSemverTag(raw)) return false
  if (isStrictSemverTag(candidateResolvedTag)) return false
  return (candidateDigest ?? '').trim().length > 0
}

type ScanSummary = DigestSnapshotUpdatedDetail['scan']

function scanHasFailures(scan: ScanSummary | null | undefined): boolean {
  if (!scan) return false
  return scan.manifestsTimeout > 0 || scan.manifestsError > 0
}

function scanIsComplete(scan: ScanSummary | null | undefined): boolean {
  if (!scan) return false
  return scan.repoTagsConsidered >= scan.repoTagsTotal
}

export function AggregateUpdatePreviewList(props: {
  items: AggregateUpdatePreviewListItem[]
  dockrevGuardHint: string
  onServiceResolvedTags?: (update: {
    stackId?: string
    serviceId: string
    resolvedTag: string | null
    resolvedTags: string[] | null
  }) => void
  onServiceCandidateResolvedTag?: (update: {
    stackId?: string
    serviceId: string
    resolvedTag: string | null
  }) => void
}) {
  type ImageOverride = { resolvedTag: string | null; resolvedTags: string[] | null }

  // ConfirmProvider snapshots modal bodies. Track local overrides so popover-triggered refreshes
  // can update sibling rows inside the same modal (raw tag row, candidate trigger, etc.).
  const [imageOverrides, setImageOverrides] = useState<Map<string, ImageOverride>>(() => new Map())
  const [candidateOverrides, setCandidateOverrides] = useState<Map<string, string | null>>(() => new Map())

  useEffect(() => {
    if (typeof window === 'undefined') return
    const onDigestSnapshotUpdated = (evt: Event) => {
      const detail =
        evt instanceof CustomEvent
          ? (evt.detail as DigestSnapshotUpdatedDetail | null)
          : null
      if (!detail) return

      const imageRepo = (detail.imageRepo ?? '').trim().toLowerCase()
      const digestNorm = normalizeDigest(detail.digest)?.toLowerCase() ?? null
      if (!imageRepo || !digestNorm) return

      const failures = scanHasFailures(detail.scan)
      const complete = scanIsComplete(detail.scan)

      setImageOverrides((prev) => {
        let changed = false
        const next = new Map(prev)

        for (const item of props.items) {
          const svc = item.svc
          const svcRepo = imageRepoFromImageRef(svc.image.ref)
          if (!svcRepo || svcRepo !== imageRepo) continue

          const currentDigest = normalizeDigest(svc.image.digest)?.toLowerCase() ?? null
          if (currentDigest !== digestNorm) continue

          const inferred = inferResolvedTagsFromSnapshot(detail.tags, svc.image.tag)
          const inferredFirst = inferred[0] ?? null
          if (!inferredFirst && (failures || !complete)) continue

          const existing = next.get(svc.id)
          const nextResolvedTags = inferred.length > 1 ? inferred : null
          if (
            existing?.resolvedTag === inferredFirst &&
            JSON.stringify(existing?.resolvedTags ?? null) === JSON.stringify(nextResolvedTags)
          ) {
            continue
          }

          changed = true
          next.set(svc.id, { resolvedTag: inferredFirst, resolvedTags: nextResolvedTags })
        }

        return changed ? next : prev
      })

      setCandidateOverrides((prev) => {
        let changed = false
        const next = new Map(prev)

        for (const item of props.items) {
          const svc = item.svc
          const svcRepo = imageRepoFromImageRef(svc.image.ref)
          if (!svcRepo || svcRepo !== imageRepo) continue

          const candidate = svc.candidate
          const candidateDigest = candidate ? normalizeDigest(candidate.digest)?.toLowerCase() ?? null : null
          if (!candidate || candidateDigest !== digestNorm) continue

          const inferred = inferResolvedTagsFromSnapshot(detail.tags, candidate.tag)
          const inferredFirst = inferred[0] ?? null
          if (!inferredFirst && (failures || !complete)) continue

          if (next.get(svc.id) === inferredFirst) continue
          changed = true
          next.set(svc.id, inferredFirst)
        }

        return changed ? next : prev
      })
    }

    window.addEventListener(DIGEST_SNAPSHOT_UPDATED_EVENT, onDigestSnapshotUpdated)
    return () => {
      window.removeEventListener(DIGEST_SNAPSHOT_UPDATED_EVENT, onDigestSnapshotUpdated)
    }
  }, [props.items])

  return (
    <div className="modalList">
      {props.items.map((item) => {
        const svcId = item.svc.id
        const imageOverride = imageOverrides.get(svcId)
        const hasCandidateOverride = candidateOverrides.has(svcId)
        const candidateOverride = candidateOverrides.get(svcId) ?? null

        const svc: Service =
          imageOverride || hasCandidateOverride
            ? {
                ...item.svc,
                image: imageOverride
                  ? {
                      ...item.svc.image,
                      resolvedTag: imageOverride.resolvedTag,
                      resolvedTags: imageOverride.resolvedTags,
                    }
                  : item.svc.image,
                candidate: item.svc.candidate
                  ? hasCandidateOverride
                    ? { ...item.svc.candidate, resolvedTag: candidateOverride }
                    : item.svc.candidate
                  : item.svc.candidate,
              }
            : item.svc

        const currentDisplayTag = formatTagDisplay(
          svc.image.tag,
          svc.image.resolvedTag,
          svc.versionInference?.status,
        )
        const inferencePending = svc.versionInference?.status === 'pending'
        const rawTagTrim = (svc.image.tag ?? '').trim()
        const showRawTag = Boolean(rawTagTrim && rawTagTrim !== currentDisplayTag)
        const candidateTag = svc.candidate?.tag && svc.candidate.tag !== '-' ? svc.candidate.tag : null
        const candidateDisplayTag = candidateTag
          ? formatCandidateTagDisplay(candidateTag, svc.candidate?.resolvedTag ?? null, svc.versionInference?.status)
          : null
        const candidatePrefetchOnMount =
          candidateTag && candidateDisplayTag
            ? shouldPrefetchFloatingCandidate(candidateTag, svc.candidate?.resolvedTag ?? null, svc.candidate?.digest ?? null)
            : false
        const semverAnomaly = isSemverDowngradeAnomaly(svc)
        const arrowPulse = inferencePending
        const img = splitImageRef(svc.image.ref)
        const dn = splitImageNameForDisplay(img.name, svc.image.tag)
        const classNames = [
          'modalListItem',
          semverAnomaly ? 'modalListItemAnomaly' : null,
          item.guardedDockrev ? 'modalListItemGuarded' : null,
        ]
          .filter(Boolean)
          .join(' ')

        return (
          <div
            key={`${item.displayName ?? svc.name}:${svc.id}`}
            className={classNames}
            aria-disabled={item.guardedDockrev ? true : undefined}
          >
            <div className="modalListLeft">
              <div className="modalListTitle">
                <span className="mono">{item.displayName ?? svc.name}</span>
                <span className="muted">{` · ${item.status}`}</span>
                {item.guardedDockrev ? (
                  <TooltipProvider delayDuration={160}>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <button
                          type="button"
                          className="modalListGuardHintTrigger"
                          aria-label="Dockrev 聚合更新保护说明"
                        >
                          <Icon icon={helpCircleOutline} className="modalListGuardHintIcon" aria-hidden="true" />
                        </button>
                      </TooltipTrigger>
                      <TooltipContent>{props.dockrevGuardHint}</TooltipContent>
                    </Tooltip>
                  </TooltipProvider>
                ) : null}
              </div>
              <div className="cellTwoLine">
                <div className="mono monoPrimary monoSplit" title={dn.suffix ? `${dn.base}${dn.suffix}` : dn.base}>
                  <span className="monoSplitBase">{dn.base}</span>
                </div>
                <div className="mono monoSecondary">{img.registry}</div>
              </div>
              {semverAnomaly ? (
                <div className="modalAnomalyNote">
                  <span className="modalAnomalyIcon" aria-hidden="true">
                    ⚠
                  </span>
                  <span>版本异常：候选版本低于当前版本</span>
                </div>
              ) : null}
            </div>
            <div className="modalListRight">
              <div className="cellTwoLine">
                <div className="versionLine">
                  <CurrentVersionPopover
                    serviceId={svc.id}
                    displayTag={currentDisplayTag}
                    imageTag={svc.image.tag}
                    imageDigest={svc.image.digest ?? null}
                    resolvedTag={svc.image.resolvedTag}
                    resolvedTags={svc.image.resolvedTags}
                    onLocalResolvedTags={(update) => {
                      setImageOverrides((prev) => {
                        const next = new Map(prev)
                        next.set(svcId, update)
                        return next
                      })
                      props.onServiceResolvedTags?.({
                        stackId: item.stackId,
                        serviceId: svcId,
                        ...update,
                      })
                    }}
                    inferenceLoading={inferencePending}
                  />
                  <span className={arrowPulse ? 'inlineIconLoading' : 'inlineIconMuted'}>
                    <svg className="inlineIcon" viewBox="0 0 16 16" aria-hidden="true" focusable="false">
                      <path d="M3 8h9" />
                      <path d="M9 4l4 4-4 4" />
                    </svg>
                  </span>
                  {candidateTag && candidateDisplayTag ? (
                    <VersionTagsPopover
                      serviceId={svc.id}
                      candidateTag={candidateTag}
                      candidateDigest={svc.candidate?.digest ?? null}
                      prefetchOnMount={candidatePrefetchOnMount}
                      onLocalResolvedTag={(resolvedTag) => {
                        setCandidateOverrides((prev) => {
                          const next = new Map(prev)
                          next.set(svcId, resolvedTag)
                          return next
                        })
                        props.onServiceCandidateResolvedTag?.({
                          stackId: item.stackId,
                          serviceId: svcId,
                          resolvedTag,
                        })
                      }}
                    >
                      {candidateDisplayTag}
                    </VersionTagsPopover>
                  ) : (
                    <span className="mono monoPrimary">-</span>
                  )}
                </div>
                {showRawTag ? (
                  <div>
                    <CurrentVersionPopover
                      serviceId={svc.id}
                      displayTag={svc.image.tag}
                      imageTag={svc.image.tag}
                      imageDigest={svc.image.digest ?? null}
                      resolvedTag={svc.image.resolvedTag}
                      resolvedTags={svc.image.resolvedTags}
                      onLocalResolvedTags={(update) => {
                        setImageOverrides((prev) => {
                          const next = new Map(prev)
                          next.set(svcId, update)
                          return next
                        })
                        props.onServiceResolvedTags?.({
                          stackId: item.stackId,
                          serviceId: svcId,
                          ...update,
                        })
                      }}
                      preferSource="rawTag"
                      triggerClassName="versionTagsTrigger mono monoSecondary"
                    >
                      {svc.image.tag}
                    </CurrentVersionPopover>
                  </div>
                ) : null}
              </div>
            </div>
          </div>
        )
      })}
    </div>
  )
}
