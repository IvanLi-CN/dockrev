import type { Decorator } from '@storybook/react'
import {
  installDockrevMockApi,
  type DockrevApiScenario,
  type DockrevMockApiOptions,
} from './dockrevMockApi'
import {
  HOMEPAGE_NAV_SNAPSHOT_KEY,
  HOMEPAGE_RESOURCE_SUMMARY_KEY,
  HOMEPAGE_SNAPSHOT_KEY,
} from '../../pages/homepageSnapshot'
import { OVERVIEW_TOOL_PANEL_STORAGE_KEY } from '../../pages/overviewToolPanelState'

export const withDockrevMockApi: Decorator = (Story, context) => {
  const scenario = (context.parameters?.dockrevApiScenario ?? 'default') as DockrevApiScenario
  const options = {
    jobsOverride: context.parameters?.dockrevJobsOverride,
    jobsEventsPayload: context.parameters?.dockrevJobsEventsPayload,
    discoveryTimelineByServiceId: context.parameters?.dockrevDiscoveryTimelineByServiceId,
    discoveryTimelineErrorServiceIds: context.parameters?.dockrevDiscoveryTimelineErrorServiceIds,
    githubReleasesByServiceId: context.parameters?.dockrevGitHubReleasesByServiceId,
    serviceOverridesById: context.parameters?.dockrevServiceOverridesById,
    serviceBackupRecordsById: context.parameters?.dockrevServiceBackupRecordsById,
    serviceLogsByServiceId: context.parameters?.dockrevServiceLogsByServiceId,
    serviceTagSuggestionsById: context.parameters?.dockrevServiceTagSuggestionsById,
    deployCheckReportOverride: context.parameters?.dockrevDeployCheckReportOverride,
    deployWelcomeOverride: context.parameters?.dockrevDeployWelcomeOverride,
    supervisorSelfUpgradeResponse: context.parameters?.dockrevSupervisorSelfUpgradeResponse,
  } satisfies DockrevMockApiOptions
  window.localStorage.removeItem(HOMEPAGE_SNAPSHOT_KEY)
  window.localStorage.removeItem(HOMEPAGE_NAV_SNAPSHOT_KEY)
  window.localStorage.removeItem(HOMEPAGE_RESOURCE_SUMMARY_KEY)
  window.localStorage.removeItem(OVERVIEW_TOOL_PANEL_STORAGE_KEY)
  const snapshotV2 = context.parameters?.dockrevHomepageSnapshot
  const navSnapshot = context.parameters?.dockrevHomepageNavSnapshot
  const resourceSnapshot = context.parameters?.dockrevHomepageResourceSummarySnapshot
  if (snapshotV2) {
    window.localStorage.setItem(HOMEPAGE_SNAPSHOT_KEY, JSON.stringify(snapshotV2))
  }
  if (navSnapshot) {
    window.localStorage.setItem(HOMEPAGE_NAV_SNAPSHOT_KEY, JSON.stringify(navSnapshot))
  }
  if (resourceSnapshot) {
    window.localStorage.setItem(HOMEPAGE_RESOURCE_SUMMARY_KEY, JSON.stringify(resourceSnapshot))
  }
  installDockrevMockApi(scenario, options)
  return Story()
}
