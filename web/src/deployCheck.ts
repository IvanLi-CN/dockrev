import {
  getDeployCheckReport,
  refreshDeployCheckReport,
  type DeployCheckReportEnvelope,
  type DeployCheckReportResponse,
} from './api'

export function hasBlockingDeployCheckFailure(report: DeployCheckReportResponse): boolean {
  const requiredCoreChecks = report.checks.filter(
    (check) => check.required && (check.group === 'core' || check.id.startsWith('core.')),
  )
  return (
    report.overall.result === 'fail' ||
    requiredCoreChecks.length === 0 ||
    requiredCoreChecks.some((check) => check.status !== 'pass')
  )
}

export function shouldKeepPollingDeployCheckReport(envelope: DeployCheckReportEnvelope): boolean {
  return envelope.status !== 'ready' || Boolean(envelope.refreshing)
}

export function shouldTriggerDeployCheckReportRefresh(envelope: DeployCheckReportEnvelope): boolean {
  return envelope.status === 'ready' && !envelope.refreshing && !envelope.lastError
}

export function shouldKeepDeployCheckLoading(envelope: DeployCheckReportEnvelope): boolean {
  return !envelope.report && shouldKeepPollingDeployCheckReport(envelope)
}

export async function settleDeployCheckReport(
  envelope: DeployCheckReportEnvelope,
): Promise<DeployCheckReportEnvelope> {
  let current = envelope
  if (current.lastError) throw new Error(current.lastError)
  while (shouldKeepPollingDeployCheckReport(current)) {
    const retryAfter = Math.max(200, current.retryAfterMs ?? 800)
    await new Promise((resolve) => window.setTimeout(resolve, retryAfter))
    current = await getDeployCheckReport()
    if (current.lastError) throw new Error(current.lastError)
  }
  return current
}

export async function refreshDeployCheckReportUntilReady(): Promise<DeployCheckReportEnvelope> {
  let envelope = await getDeployCheckReport()
  if (envelope.lastError) throw new Error(envelope.lastError)
  if (shouldTriggerDeployCheckReportRefresh(envelope)) {
    envelope = await refreshDeployCheckReport()
  }
  return settleDeployCheckReport(envelope)
}
