import type { Preview } from '@storybook/react'

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
      return Story()
    },
  ],
}

export default preview
