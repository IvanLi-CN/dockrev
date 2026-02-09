import type { StorybookConfig } from '@storybook/react-vite'

const config: StorybookConfig = {
  framework: {
    name: '@storybook/react-vite',
    options: {},
  },
  // We currently don't use MDX stories; keeping the MDX glob makes Storybook emit a noisy warning.
  stories: ['../src/**/*.stories.@(js|jsx|mjs|ts|tsx)'],
  core: {
    // Storybook's "What's new" banner is noise for our review workflow.
    disableWhatsNewNotifications: true,
  },
}

export default config
