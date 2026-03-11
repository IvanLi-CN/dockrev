import { useMemo, useState } from 'react'

import { ArrowRightIcon } from '../ui'
import { formatCandidateTagDisplay, formatCurrentTagDisplay } from '../versionDisplay'
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

export function ConfirmServiceVersionCell(props: {
  serviceId: string
  imageTag: string
  imageDigest: string | null
  resolvedTag: string | null | undefined
  resolvedTags: string[] | null | undefined
  inferenceStatus?: string | null | undefined
  candidateTag: string | null | undefined
  candidateDigest: string | null | undefined
  candidateResolvedTag: string | null | undefined
  prefetchOnMount?: boolean
  onHostResolvedTags?: (update: ResolvedTagsUpdate) => void
  onHostCandidateResolvedTag?: (resolvedTag: string | null) => void
}) {
  const {
    serviceId,
    imageTag,
    imageDigest,
    inferenceStatus,
    onHostCandidateResolvedTag,
    onHostResolvedTags,
    prefetchOnMount = false,
  } = props

  const [currentOverride, setCurrentOverride] = useState<CurrentResolvedTagsOverride | null>(null)
  const [candidateOverride, setCandidateOverride] = useState<CandidateResolvedTagOverride | null>(null)

  const currentDigestKey = useMemo(() => buildDigestKey(serviceId, imageDigest), [serviceId, imageDigest])
  const activeCurrentOverride =
    currentOverride &&
    currentOverride.key === currentDigestKey &&
    currentOverride.baseResolvedTag === normalizeTag(props.resolvedTag) &&
    tagsKey(currentOverride.baseResolvedTags) === tagsKey(props.resolvedTags)
      ? currentOverride
      : null

  const effectiveResolvedTag = activeCurrentOverride ? activeCurrentOverride.resolvedTag : props.resolvedTag ?? null
  const effectiveResolvedTags = activeCurrentOverride ? activeCurrentOverride.resolvedTags : props.resolvedTags ?? null

  const candidateDigestKey = useMemo(
    () => buildDigestKey(serviceId, props.candidateDigest),
    [serviceId, props.candidateDigest],
  )
  const activeCandidateOverride =
    candidateOverride &&
    candidateOverride.key === candidateDigestKey &&
    candidateOverride.baseResolvedTag === normalizeTag(props.candidateResolvedTag)
      ? candidateOverride
      : null
  const effectiveCandidateResolvedTag = activeCandidateOverride
    ? activeCandidateOverride.resolvedTag
    : props.candidateResolvedTag ?? null

  const inferencePending = (inferenceStatus ?? '').trim() === 'pending'
  const currentDisplayTag = useMemo(
    () => formatCurrentTagDisplay(imageTag, effectiveResolvedTag, inferenceStatus),
    [imageTag, inferenceStatus, effectiveResolvedTag],
  )

  const rawTrim = (imageTag ?? '').trim()
  const showRawTag = Boolean(rawTrim && rawTrim !== currentDisplayTag)

  const candidateTagTrim = (props.candidateTag ?? '').trim()
  const candidateTag = candidateTagTrim && candidateTagTrim !== '-' ? candidateTagTrim : null
  const candidateDisplayTag = useMemo(() => {
    if (!candidateTag) return null
    return formatCandidateTagDisplay(candidateTag, effectiveCandidateResolvedTag, inferenceStatus)
  }, [candidateTag, inferenceStatus, effectiveCandidateResolvedTag])

  const handleLocalResolvedTags = (update: ResolvedTagsUpdate) => {
    setCurrentOverride({
      key: currentDigestKey,
      baseResolvedTag: normalizeTag(props.resolvedTag),
      baseResolvedTags: normalizeTags(props.resolvedTags),
      resolvedTag: update.resolvedTag,
      resolvedTags: update.resolvedTags,
    })
    onHostResolvedTags?.(update)
  }

  const handleLocalCandidateResolvedTag = (resolvedTag: string | null) => {
    setCandidateOverride({
      key: candidateDigestKey,
      baseResolvedTag: normalizeTag(props.candidateResolvedTag),
      resolvedTag,
    })
    onHostCandidateResolvedTag?.(resolvedTag)
  }

  return (
    <div className="cellTwoLine">
      <div className="versionLine">
        <CurrentVersionPopover
          serviceId={serviceId}
          displayTag={currentDisplayTag}
          imageTag={imageTag}
          imageDigest={imageDigest}
          resolvedTag={effectiveResolvedTag}
          resolvedTags={effectiveResolvedTags}
          onLocalResolvedTags={handleLocalResolvedTags}
          inferenceLoading={inferencePending}
        />
        <span
          className={inferencePending ? 'inlineIconLoading' : 'inlineIconMuted'}
          style={inferencePending ? { margin: '0 6px' } : { opacity: 0.8, margin: '0 6px' }}
        >
          <ArrowRightIcon className="inlineIcon" />
        </span>
        {candidateTag && candidateDisplayTag ? (
          <VersionTagsPopover
            serviceId={serviceId}
            candidateTag={candidateTag}
            candidateDigest={props.candidateDigest ?? null}
            prefetchOnMount={prefetchOnMount}
            onLocalResolvedTag={handleLocalCandidateResolvedTag}
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
            serviceId={serviceId}
            displayTag={imageTag}
            imageTag={imageTag}
            imageDigest={imageDigest}
            resolvedTag={effectiveResolvedTag}
            resolvedTags={effectiveResolvedTags}
            onLocalResolvedTags={handleLocalResolvedTags}
            preferSource="rawTag"
            triggerClassName="versionTagsTrigger mono monoSecondary"
          >
            {imageTag}
          </CurrentVersionPopover>
        </div>
      ) : null}
    </div>
  )
}
