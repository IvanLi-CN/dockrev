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
  resolvedTags?: string[] | null
  reason?: string | null
  attempts: number
  retryAt?: string | null
  discoveredAt?: string | null
  lastError?: string | null
  supersededAt?: string | null
  supersededByCandidateId?: string | null
  source?: string | null
  sourceJobId?: string | null
  hydrationOrigin?: string | null
}

export type CandidateHydrationDiagnostic = {
  status: 'hydrated' | 'candidate_missing' | 'ambiguous_history' | string
  reason?: string | null
  candidateDigest?: string | null
  source?: string | null
  sourceJobId?: string | null
  hydrationOrigin?: string | null
  discoveredAt?: string | null
}

export type AutoUpdateProjection = {
  policyStatus: string
  reason?: string | null
  ruleId?: string | null
  evaluatedAt?: string | null
  policyScope?: { scopeType: string; scopeId: string } | null
  updateJobId?: string | null
}
