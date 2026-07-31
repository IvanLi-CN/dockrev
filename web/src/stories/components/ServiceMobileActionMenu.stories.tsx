import type { Meta, StoryObj } from '@storybook/react'
import { Download, Eye, Layers3, Play, RotateCcw, RotateCw, Square } from 'lucide-react'
import { expect, userEvent, within } from 'storybook/test'
import { ServiceMobileActionMenu } from '../../components/ServiceSplitActionButton'

const noop = () => {}

const meta: Meta<typeof ServiceMobileActionMenu> = {
  title: 'Components/ServiceMobileActionMenu',
  component: ServiceMobileActionMenu,
  tags: ['autodocs'],
  parameters: { layout: 'centered' },
}

export default meta
type Story = StoryObj<typeof ServiceMobileActionMenu>

export const ThreeFlatGroups: Story = {
  args: {
    groups: [
      {
        id: 'update',
        items: [
          { id: 'preview-update', label: '预览更新', icon: Eye, onSelect: noop },
          { id: 'execute-update', label: '更新', icon: Download, onSelect: noop },
          { id: 'rollback', label: '回滚', icon: RotateCcw, onSelect: noop },
        ],
      },
      {
        id: 'lifecycle',
        items: [
          { id: 'lifecycle-start', label: '启动', icon: Play, onSelect: noop },
          { id: 'lifecycle-stop', label: '停止', icon: Square, iconVariant: 'solid', onSelect: noop },
          { id: 'lifecycle-restart', label: '重启', icon: RotateCw, onSelect: noop },
        ],
      },
      {
        id: 'stack',
        items: [{ id: 'stack-details', label: 'Stack 详情', icon: Layers3, onSelect: noop }],
      },
    ],
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    const documentBody = within(canvasElement.ownerDocument.body)
    const trigger = await canvas.findByRole('button', { name: '服务操作' })
    await userEvent.click(trigger)

    const menu = await documentBody.findByRole('menu', { name: '服务操作' })
    expect(menu.querySelectorAll('[data-service-mobile-action-group]')).toHaveLength(3)
    expect(menu.querySelectorAll('[data-service-mobile-action-separator]')).toHaveLength(2)
    expect(within(menu).getAllByRole('menuitem')).toHaveLength(7)
    expect(menu.querySelector('[data-slot="dropdown-menu-sub-trigger"]')).not.toBeInTheDocument()

    await userEvent.keyboard('{ArrowDown}')
    expect(menu).toContainElement(canvasElement.ownerDocument.activeElement as HTMLElement)
    await userEvent.keyboard('{Escape}')
    await expect(trigger).toHaveFocus()
  },
}
