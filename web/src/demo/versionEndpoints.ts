import { dockrevBuildInfo } from '../buildInfo'

/**
 * Mock handlers must report the bundle's own version. A demo or Storybook
 * build has no backend, so any hardcoded literal would be a fake version.
 */
export function mockVersionResponse(): { version: string } {
  return { version: dockrevBuildInfo().version }
}
