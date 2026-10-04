import type { CleanupPreset, CleanupResourceKind } from './types'

export type CleanupEstimateBasis = 'image_unique' | 'reported_usage' | 'lower_bound' | 'unknown'

export type CleanupResourceItem = {
  resourceId: string
  kind: CleanupResourceKind
  label: string
  reason: string
  minPreset: CleanupPreset
  estimatedReclaimableBytes?: number | null
  estimateUnknown?: boolean
  estimateBasis?: CleanupEstimateBasis
}
