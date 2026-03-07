import { useState } from 'react'
import type { Meta, StoryObj } from '@storybook/react'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../../ui'

function TabsPreview() {
  const [value, setValue] = useState('cpu')
  return (
    <Tabs onValueChange={setValue} value={value} style={{ width: '100%' }}>
      <TabsList className="svcResourceTabs" aria-label="指标切换">
        <TabsTrigger className={value === 'cpu' ? 'svcResourceTab active' : 'svcResourceTab'} value="cpu">
          CPU
        </TabsTrigger>
        <TabsTrigger className={value === 'memory' ? 'svcResourceTab active' : 'svcResourceTab'} value="memory">
          内存
        </TabsTrigger>
        <TabsTrigger className={value === 'network' ? 'svcResourceTab active' : 'svcResourceTab'} value="network">
          网络
        </TabsTrigger>
      </TabsList>
      <TabsContent value="cpu">
        <div className="svcResourceChartEmpty">CPU 趋势示意</div>
      </TabsContent>
      <TabsContent value="memory">
        <div className="svcResourceChartEmpty">内存趋势示意</div>
      </TabsContent>
      <TabsContent value="network">
        <div className="svcResourceChartEmpty">网络趋势示意</div>
      </TabsContent>
    </Tabs>
  )
}

const meta: Meta<typeof TabsPreview> = {
  title: 'Components/Tabs',
  component: TabsPreview,
  tags: ['autodocs'],
}

export default meta

type Story = StoryObj<typeof TabsPreview>

export const Default: Story = {}
