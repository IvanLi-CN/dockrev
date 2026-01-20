import type { Decorator } from '@storybook/react'
import { installDockrevMockApi, type DockrevApiScenario } from './dockrevMockApi'

export const withDockrevMockApi: Decorator = (Story, context) => {
  const scenario = (context.parameters?.dockrevApiScenario ?? 'default') as DockrevApiScenario
  installDockrevMockApi(scenario)
  return Story()
}
