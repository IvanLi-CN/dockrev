import type { Meta, StoryObj } from '@storybook/react'
import { fireEvent, userEvent, within } from 'storybook/test'
import { ThemePreferenceControl } from '../../components/ThemePreferenceControl'
import { THEME_STORAGE_KEY } from '../../theme'
import { expectStory, waitForCondition } from '../pages/storyAssertions'

const meta: Meta<typeof ThemePreferenceControl> = {
  title: 'Components/ThemePreferenceControl',
  component: ThemePreferenceControl,
  decorators: [
    (Story) => (
      <div style={{ display: 'flex', width: 240, justifyContent: 'center', padding: 32 }}>
        <Story />
      </div>
    ),
  ],
}

export default meta
type Story = StoryObj<typeof ThemePreferenceControl>

function resetThemePreference() {
  try {
    window.localStorage.removeItem(THEME_STORAGE_KEY)
  } catch {
    // Storybook should still render when storage is unavailable.
  }
}

export const IconButton: Story = {
  args: { variant: 'icon' },
  beforeEach: resetThemePreference,
  play: async ({ canvasElement }) => {
    const button = canvasElement.querySelector<HTMLButtonElement>('.themePreferenceIconButton')
    expectStory(button, 'theme icon button should render')
    expectStory(button.getAttribute('aria-label')?.startsWith('主题：'), 'theme icon should expose its current preference')
    fireEvent.contextMenu(button)
    const body = within(document.body)
    await waitForCondition(() => Boolean(body.queryByRole('menu')))
    expectStory(Boolean(body.getByText('跟随系统')), 'context menu should expose system theme')
    expectStory(Boolean(body.getByText('亮色')), 'context menu should expose light theme')
    expectStory(Boolean(body.getByText('暗色')), 'context menu should expose dark theme')
    await userEvent.click(body.getByText('亮色'))
    expectStory(window.localStorage.getItem(THEME_STORAGE_KEY) === 'light', 'menu selection should persist explicit light theme')
  },
}

export const ExpandedSlider: Story = {
  args: { variant: 'segmented' },
  beforeEach: resetThemePreference,
  render: (args) => <div style={{ width: 180 }}><ThemePreferenceControl {...args} /></div>,
  play: async ({ canvasElement }) => {
    const group = canvasElement.querySelector<HTMLElement>('.themePreferenceSegmented')
    const options = canvasElement.querySelectorAll<HTMLButtonElement>('.themePreferenceSegment')
    expectStory(group?.getAttribute('role') === 'radiogroup', 'expanded theme control should be a radio group')
    expectStory(options.length === 3, 'expanded theme control should expose three icon choices')
    await userEvent.click(options[2])
    expectStory(window.localStorage.getItem(THEME_STORAGE_KEY) === 'dark', 'slider should persist the selected explicit theme')
    expectStory(options[2].getAttribute('aria-checked') === 'true', 'slider should expose selected state')
  },
}
