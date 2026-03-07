import type { Preview } from '@storybook/react'

import { TooltipProvider } from '../src/components/ui/tooltip'
import '../src/index.css'
import '../src/App.css'

const preview: Preview = {
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
      document.documentElement.dataset.theme = theme
      document.documentElement.style.colorScheme = theme
      document.documentElement.classList.toggle('dark', theme === 'dark')
      return (
        <TooltipProvider>
          <Story />
        </TooltipProvider>
      )
    },
  ],
}

export default preview
