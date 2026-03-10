import { useMemo, useState } from 'react'

import { ArrowRightIcon } from '../ui'
import { formatCandidateTagDisplay, formatCurrentTagDisplay } from '../versionDisplay'
import { CurrentVersionPopover } from './CurrentVersionPopover'
import { VersionTagsPopover } from './VersionTagsPopover'

type ResolvedTagsUpdate = {
  resolvedTag: string | null
  resolvedTags: string[] | null
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

  const [localResolvedTag, setLocalResolvedTag] = useState<string | null>(props.resolvedTag ?? null)
  const [localResolvedTags, setLocalResolvedTags] = useState<string[] | null>(props.resolvedTags ?? null)
  const [localCandidateResolvedTag, setLocalCandidateResolvedTag] = useState<string | null>(
    props.candidateResolvedTag ?? null,
  )

  const inferencePending = (inferenceStatus ?? '').trim() === 'pending'
  const currentDisplayTag = useMemo(
    () => formatCurrentTagDisplay(imageTag, localResolvedTag, inferenceStatus),
    [imageTag, inferenceStatus, localResolvedTag],
  )

  const rawTrim = (imageTag ?? '').trim()
  const showRawTag = Boolean(rawTrim && rawTrim !== currentDisplayTag)

  const candidateTagTrim = (props.candidateTag ?? '').trim()
  const candidateTag = candidateTagTrim && candidateTagTrim !== '-' ? candidateTagTrim : null
  const candidateDisplayTag = useMemo(() => {
    if (!candidateTag) return null
    return formatCandidateTagDisplay(candidateTag, localCandidateResolvedTag, inferenceStatus)
  }, [candidateTag, inferenceStatus, localCandidateResolvedTag])

  const handleLocalResolvedTags = (update: ResolvedTagsUpdate) => {
    setLocalResolvedTag(update.resolvedTag)
    setLocalResolvedTags(update.resolvedTags)
    onHostResolvedTags?.(update)
  }

  const handleLocalCandidateResolvedTag = (resolvedTag: string | null) => {
    setLocalCandidateResolvedTag(resolvedTag)
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
          resolvedTag={localResolvedTag}
          resolvedTags={localResolvedTags}
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
            resolvedTag={localResolvedTag}
            resolvedTags={localResolvedTags}
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
