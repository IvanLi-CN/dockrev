export type CandidateSettlement = {
  status: string
  rawTag?: string | null
  candidateDigest?: string | null
  resolvedVersion?: string | null
  reason?: string | null
  attempts: number
  retryAt?: string | null
  discoveredAt?: string | null
}

export type AutoUpdateProjection = {
  policyStatus: string
  reason?: string | null
  ruleId?: string | null
  evaluatedAt?: string | null
}
