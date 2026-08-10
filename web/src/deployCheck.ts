import {
  getDeployCheckReport,
  refreshDeployCheckReport,
  type DeployCheckReportEnvelope,
  type DeployCheckReportResponse,
} from './api'

export function hasBlockingDeployCheckFailure(report: DeployCheckReportResponse): boolean {
  return (
    report.overall.result === 'fail' ||
    report.checks.some((check) => check.required && check.status === 'fail')
  )
}

export function shouldKeepPollingDeployCheckReport(envelope: DeployCheckReportEnvelope): boolean {
  return envelope.status !== 'ready' || Boolean(envelope.refreshing)
}

export function shouldTriggerDeployCheckReportRefresh(envelope: DeployCheckReportEnvelope): boolean {
  return envelope.status === 'ready' && !envelope.refreshing
}

export function shouldKeepDeployCheckLoading(envelope: DeployCheckReportEnvelope): boolean {
  return !envelope.report && shouldKeepPollingDeployCheckReport(envelope)
}

export async function settleDeployCheckReport(
  envelope: DeployCheckReportEnvelope,
): Promise<DeployCheckReportEnvelope> {
  let current = envelope
  while (shouldKeepPollingDeployCheckReport(current)) {
    const retryAfter = Math.max(200, current.retryAfterMs ?? 800)
    await new Promise((resolve) => window.setTimeout(resolve, retryAfter))
    current = await getDeployCheckReport()
  }
  return current
}

export async function refreshDeployCheckReportUntilReady(): Promise<DeployCheckReportEnvelope> {
  let envelope = await getDeployCheckReport()
  if (shouldTriggerDeployCheckReportRefresh(envelope)) {
    envelope = await refreshDeployCheckReport()
  }
  return settleDeployCheckReport(envelope)
}
