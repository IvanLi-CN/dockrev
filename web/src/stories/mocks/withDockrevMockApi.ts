import type { Decorator } from '@storybook/react'
import {
  installDockrevMockApi,
  type DockrevApiScenario,
  type DockrevMockApiOptions,
} from './dockrevMockApi'
import {
  HOMEPAGE_NAV_SNAPSHOT_KEY,
  HOMEPAGE_RESOURCE_SUMMARY_KEY,
} from '../../pages/homepageSnapshot'

export const withDockrevMockApi: Decorator = (Story, context) => {
  const scenario = (context.parameters?.dockrevApiScenario ?? 'default') as DockrevApiScenario
  const options = {
    discoveryTimelineByServiceId: context.parameters?.dockrevDiscoveryTimelineByServiceId,
    discoveryTimelineErrorServiceIds: context.parameters?.dockrevDiscoveryTimelineErrorServiceIds,
    githubReleasesByServiceId: context.parameters?.dockrevGitHubReleasesByServiceId,
    serviceOverridesById: context.parameters?.dockrevServiceOverridesById,
    serviceTagSuggestionsById: context.parameters?.dockrevServiceTagSuggestionsById,
  } satisfies DockrevMockApiOptions
  window.localStorage.removeItem(HOMEPAGE_NAV_SNAPSHOT_KEY)
  window.localStorage.removeItem(HOMEPAGE_RESOURCE_SUMMARY_KEY)
  const navSnapshot = context.parameters?.dockrevHomepageNavSnapshot
  const resourceSnapshot = context.parameters?.dockrevHomepageResourceSummarySnapshot
  if (navSnapshot) {
    window.localStorage.setItem(HOMEPAGE_NAV_SNAPSHOT_KEY, JSON.stringify(navSnapshot))
  }
  if (resourceSnapshot) {
    window.localStorage.setItem(HOMEPAGE_RESOURCE_SUMMARY_KEY, JSON.stringify(resourceSnapshot))
  }
  installDockrevMockApi(scenario, options)
  return Story()
}
