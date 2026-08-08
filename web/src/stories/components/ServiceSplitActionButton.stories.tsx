import type { Meta, StoryObj } from '@storybook/react'
import { Download, Eye, Play, RotateCcw, RotateCw, Square } from 'lucide-react'
import { expect, fireEvent, userEvent, within } from 'storybook/test'
import { ServiceSplitActionButton } from '../../components/ServiceSplitActionButton'

const noop = () => {}

const meta: Meta<typeof ServiceSplitActionButton> = {
  title: 'Components/ServiceSplitActionButton',
  component: ServiceSplitActionButton,
  tags: ['autodocs'],
  parameters: { layout: 'centered' },
}

export default meta
type Story = StoryObj<typeof ServiceSplitActionButton>

export const UpdatePreferred: Story = {
  args: {
    ariaLabel: '更新操作',
    primary: { id: 'execute-update', label: '更新', icon: Download, onSelect: noop },
    items: [
      { id: 'preview-update', label: '预览更新', icon: Eye, onSelect: noop },
      { id: 'execute-update', label: '更新', icon: Download, onSelect: noop },
      { id: 'rollback', label: '回滚', icon: RotateCcw, description: '当前没有可回滚的升级记录', disabled: true, onSelect: noop },
    ],
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    const documentBody = within(canvasElement.ownerDocument.body)
    const toggle = await canvas.findByRole('button', { name: '更新操作菜单' })
    await userEvent.click(toggle)

    const menu = await documentBody.findByRole('menu', { name: '更新操作' })
    const group = await canvas.findByRole('group', { name: '更新操作' })
    expect(toggle).toHaveAttribute('data-state', 'open')
    expect(menu).toHaveClass('w-max')
    expect(menu).toHaveClass('min-w-0')
    expect(menu).not.toHaveClass('w-40')
    expect(menu.getBoundingClientRect().width).toBeGreaterThanOrEqual(group.getBoundingClientRect().width)
    expect(within(menu).getByText('回滚')).toBeInTheDocument()
    expect(within(menu).queryByText('默认')).not.toBeInTheDocument()
    expect(within(menu).queryByText('当前没有可回滚的升级记录')).not.toBeInTheDocument()
    expect(menu.querySelectorAll('.serviceSplitActionMenuItemIcon')).toHaveLength(3)

    await userEvent.keyboard('{ArrowDown}')
    expect(menu.contains(canvasElement.ownerDocument.activeElement)).toBe(true)

    await userEvent.keyboard('{Escape}')
    await expect(toggle).toHaveFocus()

    await userEvent.click(toggle)
    const reopenedMenu = await documentBody.findByRole('menu', { name: '更新操作' })
    const rollback = within(reopenedMenu).getByRole('menuitem', { name: '回滚' })
    expect(rollback).toHaveAttribute('aria-disabled', 'true')

    await userEvent.hover(rollback)
    expect(await documentBody.findByRole('tooltip')).toHaveTextContent('当前没有可回滚的升级记录')

    fireEvent.click(rollback)
    expect(await documentBody.findByTestId('service-split-toast')).toHaveTextContent('当前没有可回滚的升级记录')
  },
}

export const LifecycleUnavailable: Story = {
  args: {
    ariaLabel: '服务生命周期',
    primary: { id: 'lifecycle-stop', label: '停止', icon: Square, iconVariant: 'solid', description: '部分副本正在运行，请先处理运行态异常', disabled: true, onSelect: noop },
    items: [
      { id: 'lifecycle-start', label: '启动', icon: Play, iconVariant: 'solid', description: '部分副本正在运行，请先处理运行态异常', disabled: true, onSelect: noop },
      { id: 'lifecycle-stop', label: '停止', icon: Square, iconVariant: 'solid', description: '部分副本正在运行，请先处理运行态异常', disabled: true, onSelect: noop },
      { id: 'lifecycle-restart', label: '重启', icon: RotateCw, description: '部分副本正在运行，请先处理运行态异常', disabled: true, onSelect: noop },
    ],
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    const documentBody = within(canvasElement.ownerDocument.body)
    const toggle = await canvas.findByRole('button', { name: '服务生命周期菜单' })
    await userEvent.click(toggle)

    const menu = await documentBody.findByRole('menu', { name: '服务生命周期' })
    expect(menu.querySelector('[data-service-split-item="lifecycle-start"] .serviceSplitActionIconSolid')).toBeInTheDocument()
    expect(menu.querySelector('[data-service-split-item="lifecycle-stop"] .serviceSplitActionIconSolid')).toBeInTheDocument()
    expect(within(menu).queryByText('部分副本正在运行，请先处理运行态异常')).not.toBeInTheDocument()
  },
}

export const GroupDisabledByServiceOperation: Story = {
  args: {
    ariaLabel: '服务生命周期',
    disabled: true,
    disabledReason: '服务正在更新，完成后才能启动、停止或重启。',
    primary: { id: 'lifecycle-stop', label: '停止', icon: Square, iconVariant: 'solid', onSelect: noop },
    items: [
      { id: 'lifecycle-start', label: '启动', icon: Play, iconVariant: 'solid', disabled: true, onSelect: noop },
      { id: 'lifecycle-stop', label: '停止', icon: Square, iconVariant: 'solid', disabled: true, onSelect: noop },
      { id: 'lifecycle-restart', label: '重启', icon: RotateCw, disabled: true, onSelect: noop },
    ],
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    const documentBody = within(canvasElement.ownerDocument.body)
    const group = await canvas.findByRole('group', { name: '服务生命周期' })
    const anchor = group.closest('.serviceSplitActionDisabledAnchor')
    expect(anchor).toHaveClass('serviceSplitActionDisabledAnchor')
    expect(group).toHaveAttribute('aria-disabled', 'true')
    expect(within(group).getByRole('button', { name: '停止' })).toBeDisabled()
    expect(within(group).getByRole('button', { name: '服务生命周期菜单' })).toBeDisabled()

    await userEvent.hover(anchor as HTMLElement)
    expect(await documentBody.findByRole('tooltip')).toHaveTextContent('服务正在更新，完成后才能启动、停止或重启。')

    fireEvent.click(anchor as HTMLElement)
    expect(await documentBody.findByRole('tooltip')).toHaveTextContent('服务正在更新，完成后才能启动、停止或重启。')
    expect(documentBody.queryByRole('menu', { name: '服务生命周期' })).not.toBeInTheDocument()
  },
}
