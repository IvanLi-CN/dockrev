import type { Service } from './api'
import { normalizeDigest } from './components/digest'
import {
  formatCandidateTagDisplay,
  formatCurrentTagDisplay,
} from './versionDisplay'

export type CandidateVersionState = {
  currentDisplayTag: string
  candidateTag: string | null
  candidateDisplayTag: string | null
  inferencePending: boolean
  showRawTag: boolean
  showCandidate: boolean
  sameDisplayUpdate: boolean
}

export function resolveCandidateVersionState(
  service: Service,
): CandidateVersionState {
  const currentDisplayTag = formatCurrentTagDisplay(
    service.image.tag,
    service.image.resolvedTag,
    service.versionInference?.status,
  )
  const inferencePending = service.versionInference?.status === 'pending'
  const rawTagTrim = (service.image.tag ?? '').trim()
  const showRawTag = Boolean(rawTagTrim && rawTagTrim !== currentDisplayTag)

  const candidateTag =
    service.candidate?.tag && service.candidate.tag !== '-'
      ? service.candidate.tag
      : null
  const candidateDisplayTag = candidateTag
    ? formatCandidateTagDisplay(
        candidateTag,
        service.candidate?.resolvedTag ?? null,
        service.versionInference?.status,
      )
    : null
  const showCandidate = Boolean(candidateTag && candidateDisplayTag)

  const currentDigest = normalizeDigest(service.image.digest)
  const candidateDigest = normalizeDigest(service.candidate?.digest)
  const sameDisplayCandidate = Boolean(
    candidateDisplayTag && candidateDisplayTag === currentDisplayTag,
  )
  const sameDisplayUpdate = Boolean(
    sameDisplayCandidate && (candidateDigest ? candidateDigest !== currentDigest : true),
  )

  return {
    currentDisplayTag,
    candidateTag,
    candidateDisplayTag,
    inferencePending,
    showRawTag,
    showCandidate,
    sameDisplayUpdate,
  }
}
