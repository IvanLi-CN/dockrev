import { Icon } from '@iconify/react'
import helpCircleOutline from '@iconify-icons/mdi/help-circle-outline'

import type { Service } from '../api'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../ui'
import { isSemverDowngradeAnomaly, type RowStatus } from '../updateStatus'
import { CurrentVersionPopover } from './CurrentVersionPopover'
import { VersionTagsPopover } from './VersionTagsPopover'
import {
  formatCandidateTagDisplay,
  formatCurrentTagDisplay as formatTagDisplay,
  isStrictSemverTag,
} from '../versionDisplay'

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

export function AggregateUpdatePreviewList(props: {
  items: AggregateUpdatePreviewListItem[]
  dockrevGuardHint: string
  onServiceResolvedTags?: (update: {
    stackId?: string
    serviceId: string
    resolvedTag: string
    resolvedTags: string[] | null
  }) => void
  onServiceCandidateResolvedTag?: (update: {
    stackId?: string
    serviceId: string
    resolvedTag: string
  }) => void
}) {
  return (
    <div className="modalList">
      {props.items.map((item) => {
        const currentDisplayTag = formatTagDisplay(
          item.svc.image.tag,
          item.svc.image.resolvedTag,
          item.svc.versionInference?.status,
        )
        const inferencePending = item.svc.versionInference?.status === 'pending'
        const rawTagTrim = (item.svc.image.tag ?? '').trim()
        const showRawTag = Boolean(rawTagTrim && rawTagTrim !== currentDisplayTag)
        const candidateTag = item.svc.candidate?.tag && item.svc.candidate.tag !== '-' ? item.svc.candidate.tag : null
        const candidateDisplayTag = candidateTag
          ? formatCandidateTagDisplay(candidateTag, item.svc.candidate?.resolvedTag ?? null, item.svc.versionInference?.status)
          : null
        const candidatePrefetchOnMount =
          candidateTag && candidateDisplayTag
            ? shouldPrefetchFloatingCandidate(candidateTag, item.svc.candidate?.resolvedTag ?? null, item.svc.candidate?.digest ?? null)
            : false
        const semverAnomaly = isSemverDowngradeAnomaly(item.svc)
        const arrowPulse = inferencePending
        const img = splitImageRef(item.svc.image.ref)
        const dn = splitImageNameForDisplay(img.name, item.svc.image.tag)
        const classNames = [
          'modalListItem',
          semverAnomaly ? 'modalListItemAnomaly' : null,
          item.guardedDockrev ? 'modalListItemGuarded' : null,
        ]
          .filter(Boolean)
          .join(' ')

        return (
          <div
            key={`${item.displayName ?? item.svc.name}:${item.svc.id}`}
            className={classNames}
            aria-disabled={item.guardedDockrev ? true : undefined}
          >
            <div className="modalListLeft">
              <div className="modalListTitle">
                <span className="mono">{item.displayName ?? item.svc.name}</span>
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
                    serviceId={item.svc.id}
                    displayTag={currentDisplayTag}
                    imageTag={item.svc.image.tag}
                    imageDigest={item.svc.image.digest ?? null}
                    resolvedTag={item.svc.image.resolvedTag}
                    resolvedTags={item.svc.image.resolvedTags}
                    onLocalResolvedTags={
                      props.onServiceResolvedTags
                        ? (update) => {
                            props.onServiceResolvedTags?.({
                              stackId: item.stackId,
                              serviceId: item.svc.id,
                              ...update,
                            })
                          }
                        : undefined
                    }
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
                      serviceId={item.svc.id}
                      candidateTag={candidateTag}
                      candidateDigest={item.svc.candidate?.digest ?? null}
                      prefetchOnMount={candidatePrefetchOnMount}
                      onLocalResolvedTag={
                        props.onServiceCandidateResolvedTag
                          ? (resolvedTag) => {
                              props.onServiceCandidateResolvedTag?.({
                                stackId: item.stackId,
                                serviceId: item.svc.id,
                                resolvedTag,
                              })
                            }
                          : undefined
                      }
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
                      serviceId={item.svc.id}
                      displayTag={item.svc.image.tag}
                      imageTag={item.svc.image.tag}
                      imageDigest={item.svc.image.digest ?? null}
                      resolvedTag={item.svc.image.resolvedTag}
                      resolvedTags={item.svc.image.resolvedTags}
                      onLocalResolvedTags={
                        props.onServiceResolvedTags
                          ? (update) => {
                              props.onServiceResolvedTags?.({
                                stackId: item.stackId,
                                serviceId: item.svc.id,
                                ...update,
                              })
                            }
                          : undefined
                      }
                      preferSource="rawTag"
                      triggerClassName="versionTagsTrigger mono monoSecondary"
                    >
                      {item.svc.image.tag}
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
