import type { Meta, StoryObj } from '@storybook/react'
import { Mono, SectionTitle } from '../../ui'

const meta: Meta = {
  title: 'Components/Typography',
}

export default meta
type Story = StoryObj

export const MonoText: Story = {
  render: () => {
    return (
      <div style={{ display: 'grid', gap: 8 }}>
        <div>
          <Mono>ghcr.io/ivan/dockrev</Mono>
        </div>
        <div>
          <Mono>sha256:0123456789abcdef</Mono>
        </div>
      </div>
    )
  },
}

export const SectionTitles: Story = {
  render: () => {
    return (
      <div style={{ display: 'grid', gap: 8 }}>
        <SectionTitle>基本信息</SectionTitle>
        <SectionTitle>高级设置</SectionTitle>
      </div>
    )
  },
}
