export type VersionInferenceState = {
  status: 'ready' | 'pending' | string
  reason?: string | null
  checkedAt?: string | null
  retryAt?: string | null
  resolvedTag?: string | null
  unresolved?: boolean | null
}

export type CandidateSettlement = {
  status: string
  rawTag?: string | null
  candidateDigest?: string | null
  resolvedVersion?: string | null
  reason?: string | null
  attempts: number
  retryAt?: string | null
  discoveredAt?: string | null
  lastError?: string | null
  supersededAt?: string | null
  supersededByCandidateId?: string | null
}

export type AutoUpdateProjection = {
  policyStatus: string
  reason?: string | null
  ruleId?: string | null
  evaluatedAt?: string | null
  policyScope?: { scopeType: string; scopeId: string } | null
  updateJobId?: string | null
}
