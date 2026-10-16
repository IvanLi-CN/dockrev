import {
  getDeployCheckReport,
  type DeployCheckReportEnvelope,
  type DeployCheckReportResponse,
} from './api'
import { createContext, createElement, useContext, type ReactNode } from 'react'

export type DeployCheckOperation = 'update' | 'rollback' | 'start' | 'stop' | 'restart' | 'cleanup'

const DeployCheckReportContext = createContext<DeployCheckReportResponse | null>(null)

export function DeployCheckProvider(props: { report: DeployCheckReportResponse | null; children: ReactNode }) {
  return createElement(DeployCheckReportContext.Provider, { value: props.report }, props.children)
}

export function useDeployCheckReport(): DeployCheckReportResponse | null {
  return useContext(DeployCheckReportContext)
}

export function deployCheckOperationImpactReason(
  report: DeployCheckReportResponse | null,
  operation: DeployCheckOperation,
  objectId?: string | null | readonly string[],
): string | null {
  if (!report) return null
  const objectIds = Array.isArray(objectId) ? objectId : objectId ? [objectId] : []
  for (const impact of report.operationImpacts ?? []) {
    if (!impact.operations.includes(operation)) continue
    if (impact.scope === 'global') return impact.reason
    if (
      objectIds.length > 0 &&
      (impact.scope === 'affected_projects' || impact.scope === 'affected_services') &&
      impact.objectIds.some((candidate) => objectIds.includes(candidate))
    ) {
      return impact.reason
    }
  }
  return null
}

export function mergeDeployCheckOperationReason(
  existingReason: string | null | undefined,
  report: DeployCheckReportResponse | null,
  operation: DeployCheckOperation,
  objectId?: string | null | readonly string[],
): string | undefined {
  return existingReason ?? deployCheckOperationImpactReason(report, operation, objectId) ?? undefined
}

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
