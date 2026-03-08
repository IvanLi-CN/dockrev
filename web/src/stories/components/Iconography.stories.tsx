import type { Meta, StoryObj } from '@storybook/react'
import { ArrowRightIcon, GitHubIcon, RefreshIcon, TrashIcon } from '../../ui'

function IconographyCatalog() {
  const items = [
    { name: 'ArrowRightIcon', icon: <ArrowRightIcon className="inlineIcon" /> },
    { name: 'RefreshIcon', icon: <RefreshIcon className="inlineIcon" /> },
    { name: 'TrashIcon', icon: <TrashIcon className="inlineIcon" /> },
    { name: 'GitHubIcon', icon: <GitHubIcon className="inlineIcon" /> },
  ]

  return (
    <div className="card" style={{ width: 520 }}>
      <div className="title">Iconography</div>
      <div className="muted" style={{ marginTop: 6 }}>共享图标 helper 与常见内联用法。</div>
      <div style={{ marginTop: 14, display: 'grid', gap: 10 }}>
        {items.map((item) => (
          <div key={item.name} style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
            <span style={{ display: 'inline-flex', width: 24, justifyContent: 'center' }}>{item.icon}</span>
            <span className="mono">{item.name}</span>
          </div>
        ))}
      </div>
    </div>
  )
}

const meta: Meta<typeof IconographyCatalog> = {
  title: 'Components/Iconography',
  component: IconographyCatalog,
}

export default meta
type Story = StoryObj<typeof IconographyCatalog>

export const Catalog: Story = {}
