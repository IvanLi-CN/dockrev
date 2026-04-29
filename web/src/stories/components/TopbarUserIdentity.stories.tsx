import type { ComponentProps } from 'react'
import type { Meta, StoryObj } from '@storybook/react'
import { TopbarUserIdentity } from '../../components/TopbarUserIdentity'
import { buildTopbarAuthIdentityFromSettings } from '../../topbarAuthIdentity'

function expectStory(condition: unknown, message: string): asserts condition {
  if (!condition) throw new globalThis.Error(message)
}

function StorySurface(props: {
  authIdentity: ComponentProps<typeof TopbarUserIdentity>['authIdentity']
  width?: number
}) {
  return (
    <div
      style={{
        width: props.width ?? 760,
        padding: 20,
        borderRadius: 20,
        background: 'var(--panel2)',
        border: '1px solid var(--borderColor)',
        display: 'flex',
        justifyContent: 'flex-end',
      }}
    >
      <TopbarUserIdentity authIdentity={props.authIdentity} />
    </div>
  )
}

const meta: Meta<typeof StorySurface> = {
  title: 'Components/TopbarUserIdentity',
  component: StorySurface,
  tags: ['autodocs'],
}

export default meta

type Story = StoryObj<typeof StorySurface>

export const CurrentUser: Story = {
  args: {
    authIdentity: buildTopbarAuthIdentityFromSettings({
      allowAnonymousInDev: true,
      allowedGroupMasked: 'o**s',
      allowedUserMasked: 'al***ce',
      authorizationMode: 'user_or_group',
      currentGroups: ['o**s'],
      currentUser: 'alice',
      forwardHeaderName: 'X-Forwarded-User',
      groupHeaderName: 'Remote-Groups',
      matchedBy: 'user',
    }),
  },
  play: async ({ canvasElement }) => {
    const trigger = canvasElement.querySelector<HTMLButtonElement>('.topbarUserTrigger')
    expectStory(trigger?.textContent?.includes('alice'), 'topbar user trigger should show the current user')
    trigger?.click()
    await new Promise((resolve) => setTimeout(resolve, 160))

    const doc = canvasElement.ownerDocument
    const popover = doc.querySelector<HTMLElement>('.topbarUserPopover')
    expectStory(popover, 'topbar identity popover should open after clicking trigger')
    expectStory(popover?.textContent?.includes('当前用户'), 'topbar identity popover should render the current user row')
    expectStory(popover?.textContent?.includes('用户或组任一命中'), 'topbar identity popover should map authorization mode')
  },
}

export const StateGallery: Story = {
  render: () => {
    const states = [
      {
        key: 'user',
        label: '用户命中',
        authIdentity: buildTopbarAuthIdentityFromSettings({
          allowAnonymousInDev: true,
          allowedGroupMasked: 'o**s',
          allowedUserMasked: 'al***ce',
          authorizationMode: 'user_or_group',
          currentGroups: ['o**s'],
          currentUser: 'alice',
          forwardHeaderName: 'X-Forwarded-User',
          groupHeaderName: 'Remote-Groups',
          matchedBy: 'user',
        }),
      },
      {
        key: 'group',
        label: '组命中',
        authIdentity: buildTopbarAuthIdentityFromSettings({
          allowAnonymousInDev: false,
          allowedGroupMasked: 'o**s',
          allowedUserMasked: null,
          authorizationMode: 'group_only',
          currentGroups: ['ops', 'platform'],
          currentUser: null,
          forwardHeaderName: 'X-Forwarded-User',
          groupHeaderName: 'Remote-Groups',
          matchedBy: 'group',
        }),
      },
      {
        key: 'anonymous',
        label: '匿名开发',
        authIdentity: buildTopbarAuthIdentityFromSettings({
          allowAnonymousInDev: true,
          allowedGroupMasked: null,
          allowedUserMasked: null,
          authorizationMode: 'unconfigured',
          currentGroups: [],
          currentUser: null,
          forwardHeaderName: 'X-Forwarded-User',
          groupHeaderName: 'Remote-Groups',
          matchedBy: 'anonymous_dev',
        }),
      },
    ]

    return (
      <div style={{ display: 'grid', gap: 16 }}>
        {states.map((state) => (
          <div key={state.key} style={{ display: 'grid', gap: 8 }}>
            <div className="label">{state.label}</div>
            <StorySurface authIdentity={state.authIdentity} />
          </div>
        ))}
      </div>
    )
  },
}

export const AvatarImage: Story = {
  args: {
    authIdentity: buildTopbarAuthIdentityFromSettings({
      allowAnonymousInDev: true,
      allowedGroupMasked: 'o**s',
      allowedUserMasked: 'al***ce',
      authorizationMode: 'user_or_group',
      avatarUrl: '/brand-mark.png',
      currentGroups: ['o**s'],
      currentUser: 'alice',
      forwardHeaderName: 'X-Forwarded-User',
      groupHeaderName: 'Remote-Groups',
      matchedBy: 'user',
    }),
  },
  play: async ({ canvasElement }) => {
    const avatar = canvasElement.querySelector<HTMLImageElement>('.topbarUserAvatarImage')
    expectStory(avatar?.getAttribute('src') === '/brand-mark.png', 'topbar user trigger should render avatar URL')
  },
}

export const MobileCompact: Story = {
  args: {
    width: 390,
    authIdentity: buildTopbarAuthIdentityFromSettings({
      allowAnonymousInDev: true,
      allowedGroupMasked: 'o**s',
      allowedUserMasked: 'al***ce',
      authorizationMode: 'user_or_group',
      currentGroups: ['o**s'],
      currentUser: 'alice-with-a-very-long-name',
      forwardHeaderName: 'X-Forwarded-User',
      groupHeaderName: 'Remote-Groups',
      matchedBy: 'user',
    }),
  },
}
