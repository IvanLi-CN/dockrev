import type { Meta, StoryObj } from '@storybook/react'
import { useEffect } from 'react'
import App from '../../App'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

function LocationReset(props: { pathname: string }) {
  useEffect(() => {
    const normalized = props.pathname.startsWith('/') ? props.pathname : `/${props.pathname}`
    window.location.hash = `#${normalized}`
  }, [props.pathname])
  return null
}

const meta: Meta<typeof App> = {
  title: 'Pages/InteractiveApp',
  component: App,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof App>

export const Dashboard: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: () => {
    return (
      <>
        <LocationReset pathname="/" />
        <App />
      </>
    )
  },
}

export const Queue: Story = {
  parameters: { dockrevApiScenario: 'queue-mixed' },
  render: () => {
    return (
      <>
        <LocationReset pathname="/queue" />
        <App />
      </>
    )
  },
}

export const Services: Story = {
  parameters: { dockrevApiScenario: 'multi-stack-mixed' },
  render: () => {
    return (
      <>
        <LocationReset pathname="/services" />
        <App />
      </>
    )
  },
}

export const ServiceDetail: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: () => {
    return (
      <>
        <LocationReset pathname="/services/stack-prod/svc-prod-api" />
        <App />
      </>
    )
  },
}

export const Settings: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => {
    return (
      <>
        <LocationReset pathname="/settings" />
        <App />
      </>
    )
  },
}
