import {
  getDeployCheckReport,
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

export async function loadDeployCheckReport(): Promise<DeployCheckReportEnvelope> {
  return getDeployCheckReport()
}
