export type DeployCheckStatus = 'pass' | 'fail' | 'na'
export type DeployCheckNaReason = 'disabled_by_switch' | 'missing_prerequisite' | 'not_applicable'
export type DeployCheckGroup = 'core' | 'feature' | string
export type DeployCheckStartupStatus = 'pass' | 'fail' | 'settled'

export type DeployCheckOperationImpact = {
  checkId: string
  scope: string
  objectIds: string[]
  operations: string[]
  reason: string
}

export type DeployCheckItem = {
  id: string
  title: string
  group: DeployCheckGroup
  required: boolean
  status: DeployCheckStatus
  naReason?: DeployCheckNaReason
  summary: string
  impact: string
  evidence: string
  recommendation: string
}

export type DeployCheckReportResponse = {
  overall: {
    result: 'pass' | 'fail'
    blockingCheckIds: string[]
    summary: string
  }
  generatedAt: string
  checks: DeployCheckItem[]
  serverBootId?: string | null
  reportBootId?: string | null
  reportRevision?: number | null
  startupStatus?: DeployCheckStartupStatus | null
  startupFailureKey?: string | null
  operationImpacts?: DeployCheckOperationImpact[]
}

export type DeployCheckReportEnvelope = {
  status: 'pending' | 'ready'
  refreshing?: boolean
  retryAfterMs?: number | null
  lastError?: string | null
  report?: DeployCheckReportResponse | null
}

export type DeployWelcomeResponse = {
  neverAutoOpen: boolean
  updatedAt?: string | null
}
