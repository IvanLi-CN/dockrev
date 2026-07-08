import type { Preview } from '@storybook/react'
import { themes } from 'storybook/theming'

import { PwaStatusMockProvider } from '../src/pwaStatus'
import { TooltipProvider } from '../src/components/ui/tooltip'
import '../src/index.css'
import '../src/App.css'

const preview: Preview = {
  parameters: {
    backgrounds: {
      options: {
        dark: { name: 'dark', value: '#061227' },
        light: { name: 'light', value: '#f5f7fb' },
      },
    },
    docs: {
      theme: themes.dark,
    },
  },
  initialGlobals: {
    backgrounds: { value: 'dark' },
    theme: 'dark',
  },
  globalTypes: {
    theme: {
      description: 'Theme',
      defaultValue: 'dark',
      toolbar: {
        title: 'Theme',
        items: [
          { value: 'dark', title: 'dark' },
          { value: 'light', title: 'light' },
        ],
      },
    },
  },
  decorators: [
    (Story, context) => {
      const theme = context.globals.theme === 'light' ? 'light' : 'dark'
      const pwaStatus = context.parameters?.pwaStatus
      document.documentElement.dataset.theme = theme
      document.documentElement.style.colorScheme = theme
      document.documentElement.classList.toggle('dark', theme === 'dark')
      return (
        <PwaStatusMockProvider value={pwaStatus}>
          <TooltipProvider>
            <Story />
          </TooltipProvider>
        </PwaStatusMockProvider>
      )
    },
  ],
}

export default preview
