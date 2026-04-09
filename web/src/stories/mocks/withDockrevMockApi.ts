import type { Decorator } from '@storybook/react'
import {
  installDockrevMockApi,
  type DockrevApiScenario,
  type DockrevMockApiOptions,
} from './dockrevMockApi'

export const withDockrevMockApi: Decorator = (Story, context) => {
  const scenario = (context.parameters?.dockrevApiScenario ?? 'default') as DockrevApiScenario
  const options = {
    discoveryTimelineByServiceId: context.parameters?.dockrevDiscoveryTimelineByServiceId,
    githubReleasesByServiceId: context.parameters?.dockrevGitHubReleasesByServiceId,
    serviceOverridesById: context.parameters?.dockrevServiceOverridesById,
  } satisfies DockrevMockApiOptions
  installDockrevMockApi(scenario, options)
  return Story()
}
