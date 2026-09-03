import type { Meta, StoryObj } from '@storybook/react'
import { NotFoundView } from '../../components/NotFoundView'

const meta = {
  title: 'Components/NotFoundView',
  component: NotFoundView,
  parameters: { layout: 'fullscreen' },
} satisfies Meta<typeof NotFoundView>

export default meta
type Story = StoryObj<typeof meta>

export const UnknownDocument: Story = {
  args: { pathname: '/made-up-deep-link' },
}
