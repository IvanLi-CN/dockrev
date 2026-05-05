import { useMemo, useState } from 'react'

import type { Service } from '../api'
import { normalizeDigest } from './digest'
import { ImageLinkIcons, splitImageNameForDisplay, splitImageRef } from '../imageLinks'
import { isSemverDowngradeAnomaly, serviceRowStatus } from '../updateStatus'
import { formatCandidateTagDisplay, formatCurrentTagDisplay, isStrictSemverTag } from '../versionDisplay'
import { Mono } from '../ui'
import { CurrentVersionPopover } from './CurrentVersionPopover'
import { VersionTagsPopover } from './VersionTagsPopover'

type ResolvedTagsUpdate = {
  resolvedTag: string | null
  resolvedTags: string[] | null
}

type CurrentResolvedTagsOverride = {
  key: string
  baseResolvedTag: string | null
  baseResolvedTags: string[] | null
  resolvedTag: string | null
  resolvedTags: string[] | null
}

type CandidateResolvedTagOverride = {
  key: string
  baseResolvedTag: string | null
  resolvedTag: string | null
}

type ConfirmSignalBadgeTone = 'action' | 'guard' | 'warn' | 'bad' | 'neutral'

function normalizeTag(value: string | null | undefined): string | null {
  const trimmed = (value ?? '').trim()
  return trimmed ? trimmed : null
}

function normalizeTags(value: string[] | null | undefined): string[] | null {
  return value == null ? null : value.map((tag) => tag.trim())
}

function tagsKey(value: string[] | null | undefined): string {
  return JSON.stringify(normalizeTags(value))
}

function buildDigestKey(serviceId: string, digest: string | null | undefined): string {
  return `${serviceId}:${(digest ?? '').trim()}`
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

function statusBadgeTone(status: string): ConfirmSignalBadgeTone {
  if (status === 'updatable') return 'action'
  if (status === 'hint') return 'warn'
  if (status === 'archMismatch' || status === 'blocked') return 'bad'
  if (status === 'ok') return 'neutral'
  return 'neutral'
}

function ConfirmSignalBadge(props: { tone: ConfirmSignalBadgeTone; label: string }) {
  return (
    <span className={`confirmSignalBadge confirmSignalBadge-${props.tone}`}>
      <span className="confirmSignalBadgeDot" aria-hidden="true" />
      <span className="mono">{props.label}</span>
    </span>
  )
}

export function ServiceUpdateConfirmDetails(props: {
  service: Service
  status?: string
  onHostResolvedTags?: (update: ResolvedTagsUpdate) => void
  onHostCandidateResolvedTag?: (resolvedTag: string | null) => void
}) {
  const { service, onHostCandidateResolvedTag, onHostResolvedTags } = props
  const [currentOverride, setCurrentOverride] = useState<CurrentResolvedTagsOverride | null>(null)
  const [candidateOverride, setCandidateOverride] = useState<CandidateResolvedTagOverride | null>(null)

  const currentDigestKey = useMemo(
    () => buildDigestKey(service.id, service.image.digest ?? null),
    [service.id, service.image.digest],
  )
  const activeCurrentOverride =
    currentOverride &&
    currentOverride.key === currentDigestKey &&
    currentOverride.baseResolvedTag === normalizeTag(service.image.resolvedTag) &&
    tagsKey(currentOverride.baseResolvedTags) === tagsKey(service.image.resolvedTags)
      ? currentOverride
      : null
  const effectiveResolvedTag = activeCurrentOverride
    ? activeCurrentOverride.resolvedTag
    : service.image.resolvedTag ?? null
  const effectiveResolvedTags = activeCurrentOverride
    ? activeCurrentOverride.resolvedTags
    : service.image.resolvedTags ?? null

  const candidateDigestKey = useMemo(
    () => buildDigestKey(service.id, service.candidate?.digest ?? null),
    [service.id, service.candidate?.digest],
  )
  const activeCandidateOverride =
    candidateOverride &&
    candidateOverride.key === candidateDigestKey &&
    candidateOverride.baseResolvedTag === normalizeTag(service.candidate?.resolvedTag)
      ? candidateOverride
      : null
  const effectiveCandidateResolvedTag = activeCandidateOverride
    ? activeCandidateOverride.resolvedTag
    : service.candidate?.resolvedTag ?? null

  const currentDisplayTag = formatCurrentTagDisplay(
    service.image.tag,
    effectiveResolvedTag,
    service.versionInference?.status,
  )
  const rawTagTrim = service.image.tag.trim()
  const showRawTag = Boolean(rawTagTrim && rawTagTrim !== currentDisplayTag)
  const candidateTag = service.candidate?.tag && service.candidate.tag !== '-' ? service.candidate.tag : null
  const candidateDisplayTag = candidateTag
    ? formatCandidateTagDisplay(candidateTag, effectiveCandidateResolvedTag, service.versionInference?.status)
    : null
  const currentDigest = normalizeDigest(service.image.digest)
  const candidateDigest = normalizeDigest(service.candidate?.digest)
  const sameDisplayUpdate = Boolean(
    candidateDisplayTag && candidateDisplayTag === currentDisplayTag && (candidateDigest ? candidateDigest !== currentDigest : true),
  )
  const inferencePending = service.versionInference?.status === 'pending'
  const candidatePrefetchOnMount = shouldPrefetchFloatingCandidate(
    candidateTag,
    effectiveCandidateResolvedTag,
    candidateDigest,
  )
  const semverDowngradeAnomaly = isSemverDowngradeAnomaly(service)
  const image = splitImageRef(service.image.ref)
  const displayName = splitImageNameForDisplay(image.name, service.image.tag)
  const status = props.status ?? serviceRowStatus(service)

  const handleLocalResolvedTags = (update: ResolvedTagsUpdate) => {
    setCurrentOverride({
      key: currentDigestKey,
      baseResolvedTag: normalizeTag(service.image.resolvedTag),
      baseResolvedTags: normalizeTags(service.image.resolvedTags),
      resolvedTag: update.resolvedTag,
      resolvedTags: update.resolvedTags,
    })
    onHostResolvedTags?.(update)
  }

  const handleLocalCandidateResolvedTag = (resolvedTag: string | null) => {
    setCandidateOverride({
      key: candidateDigestKey,
      baseResolvedTag: normalizeTag(service.candidate?.resolvedTag),
      resolvedTag,
    })
    onHostCandidateResolvedTag?.(resolvedTag)
  }

  return (
    <>
      <div className="modalLead">将对该服务执行更新（apply）。</div>
      <div className="modalKvGrid">
        <div className="modalKvLabel">镜像</div>
        <div className="modalKvValue">
          <div className="modalValueStack">
            <div className="mono monoPrimary monoSplit imageLinkRow">
              <span className="monoSplitBase">
                {displayName.suffix ? `${displayName.base}${displayName.suffix}` : displayName.base}
              </span>
              <ImageLinkIcons imageRef={service.image.ref} repoUrl={service.settings.repoUrl} />
            </div>
            <div className="mono monoSecondary">{image.registry}</div>
          </div>
        </div>
        <div className="modalKvLabel">版本</div>
        <div className="modalKvValue">
          <div className="modalValueStack">
            <div className="versionLine">
              <CurrentVersionPopover
                serviceId={service.id}
                displayTag={currentDisplayTag}
                imageTag={service.image.tag}
                imageDigest={service.image.digest ?? null}
                resolvedTag={effectiveResolvedTag}
                resolvedTags={effectiveResolvedTags}
                onLocalResolvedTags={handleLocalResolvedTags}
                inferenceLoading={inferencePending}
              />
              <span className={inferencePending ? 'inlineIconLoading' : 'inlineIconMuted'}>
                <svg className="inlineIcon" viewBox="0 0 16 16" aria-hidden="true" focusable="false">
                  <path d="M3 8h9" />
                  <path d="M9 4l4 4-4 4" />
                </svg>
              </span>
              {candidateTag && candidateDisplayTag ? (
                <>
                  <VersionTagsPopover
                    serviceId={service.id}
                    candidateTag={candidateTag}
                    candidateDigest={service.candidate?.digest ?? null}
                    prefetchOnMount={candidatePrefetchOnMount}
                    onLocalResolvedTag={handleLocalCandidateResolvedTag}
                  >
                    {candidateDisplayTag}
                  </VersionTagsPopover>
                  {sameDisplayUpdate ? <span className="versionInlineHint">同标签新 digest</span> : null}
                </>
              ) : (
                <span className="mono monoPrimary">-</span>
              )}
            </div>
            {showRawTag ? (
              <CurrentVersionPopover
                serviceId={service.id}
                displayTag={service.image.tag}
                imageTag={service.image.tag}
                imageDigest={service.image.digest ?? null}
                resolvedTag={effectiveResolvedTag}
                resolvedTags={effectiveResolvedTags}
                onLocalResolvedTags={handleLocalResolvedTags}
                preferSource="rawTag"
                triggerClassName="versionTagsTrigger mono monoSecondary"
              >
                {service.image.tag}
              </CurrentVersionPopover>
            ) : null}
          </div>
        </div>
        <div className="modalKvLabel">目标 digest</div>
        <div className="modalKvValue">
          {candidateDigest ? (
            <span className="mono">{candidateDigest}</span>
          ) : (
            <Mono>-</Mono>
          )}
        </div>
        <div className="modalKvLabel">状态</div>
        <div className="modalKvValue">
          <ConfirmSignalBadge tone={statusBadgeTone(status)} label={status} />
        </div>
        {semverDowngradeAnomaly ? (
          <>
            <div className="modalKvLabel">版本异常</div>
            <div className="modalKvValue">
              <Mono>候选版本低于当前版本（仍允许手动更新）</Mono>
            </div>
          </>
        ) : null}
        <div className="modalKvLabel">架构策略</div>
        <div className="modalKvValue">
          <ConfirmSignalBadge tone="guard" label="disallow" />
        </div>
      </div>
      <div className="modalDivider" />
    </>
  )
}
