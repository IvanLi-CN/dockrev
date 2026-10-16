import {
  getDeployCheckReport,
  type DeployCheckReportEnvelope,
  type DeployCheckReportResponse,
} from './api'

export function hasBlockingDeployCheckFailure(report: DeployCheckReportResponse): boolean {
  if (report.startupStatus) {
    return report.startupStatus === 'fail' || (report.operationImpacts ?? []).length > 0
  }
  const requiredCoreChecks = report.checks.filter(
    (check) => check.required && (check.group === 'core' || check.id.startsWith('core.')),
  )
  return requiredCoreChecks.some((check) => check.status !== 'pass')
}

export function hasActiveCoreFailure(report: DeployCheckReportResponse): boolean {
  if (report.startupStatus === 'fail') return true
  return (report.operationImpacts ?? []).length > 0
}

export function hasCurrentStartupCoreFailure(report: DeployCheckReportResponse): boolean {
  return (
    report.startupStatus === 'fail' &&
    Boolean(report.startupFailureKey) &&
    Boolean(report.serverBootId) &&
    report.reportBootId === report.serverBootId
  )
}

export async function loadDeployCheckReport(): Promise<DeployCheckReportEnvelope> {
  return getDeployCheckReport()
}
