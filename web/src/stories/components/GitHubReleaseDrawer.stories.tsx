import type { CSSProperties, ReactNode } from 'react'
import type { Meta, StoryObj } from '@storybook/react'

import { GitHubReleaseDrawer } from '../../components/GitHubReleaseDrawer'
import type { DockrevMockGitHubReleasesDataset } from '../mocks/dockrevMockApi'
import { buildFixture } from '../mocks/dockrevMockApi/fixturesMisc'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const STORY_REPO = {
  fullName: 'ivanli-cn/codex-vibe-monitor',
  htmlUrl: 'https://github.com/ivanli-cn/codex-vibe-monitor',
} as const

const RELEASE_NOTE_SETS = [
  {
    summary: '集中收口版本发现时间线和发布说明之间的跳转链路。',
    highlights: [
      '让发现时间线里的候选版本可以直接跳到 GitHub Releases 抽屉。',
      '把最近 20 条发布记录的拉取节奏和前端虚拟滚动对齐，减少首屏跳动。',
    ],
    fixes: [
      '修复定位旧版本时重复追加分页数据导致的高亮错位。',
      '统一候选版本与历史版本的 tag 规范化匹配（v 前缀 / 无前缀）。',
    ],
    ops: ['建议部署前确认 GitHub PAT 仍具备读取私有仓库 Releases 的权限。'],
  },
  {
    summary: '这版重点在稳定 GitHub API 访问路径和异常提示。',
    highlights: [
      'PAT 存在时优先走认证访问，匿名访问只在缺少 PAT 时兜底。',
      '为匿名限流和权限不足增加结构化错误，避免 UI 落成“未知失败”。',
    ],
    fixes: [
      '修复 locate 结果命中后，URL 与抽屉状态偶发不同步的问题。',
      '补齐 unsupported repo 场景，显式阻断非 GitHub source。',
    ],
    ops: ['若使用反向代理，请保留 GitHub 返回的状态码与错误消息。'],
  },
  {
    summary: '聚焦较新的 patch 版本，主要处理发布说明可读性和滚动反馈。',
    highlights: [
      '抽屉内为目标版本增加高亮和滚动动画，帮助快速建立位置感。',
      '分页列表切换为更稳定的动态测量虚拟滚动，长说明也能平滑浏览。',
    ],
    fixes: [
      '修复 Release body 长度差异大时的估算高度抖动。',
      '改进“前 50 条内未找到”提示，明确反馈扫描窗口大小。',
    ],
    ops: ['发布后建议观察前 24 小时 locate 请求量与 GitHub rate limit 余量。'],
  },
  {
    summary: '一次偏运维的稳定性小版本，回收了几个边角 case。',
    highlights: [
      '优化了抽屉关闭后的 history 行为，避免留下多余 URL 状态。',
      '将缺失 repoUrl 时的只读推断结果纳入同一展示路径。',
    ],
    fixes: [
      '修复匿名访问 public repo 时 PAT 回退判断过于保守的问题。',
      '统一发布记录外链、发布时间和 tag 名称的展示格式。',
    ],
    ops: ['如果需要分享链接，请确认 releaseDrawer/releaseServiceId/releaseVersion 三个 query 已被保留。'],
  },
] as const

const HOUR_MS = 60 * 60 * 1000
const shellStyle: CSSProperties = {
  minHeight: '100vh',
  padding: '24px',
  boxSizing: 'border-box',
  background: 'var(--bg-layered)',
}

const shellFrameStyle: CSSProperties = {
  minHeight: 'calc(100vh - 48px)',
  borderRadius: '6px',
  border: '1px solid var(--borderColor)',
  background: 'var(--panel)',
  boxShadow: 'var(--shadow-card)',
  overflow: 'hidden',
}

const shellHeaderStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: '16px',
  padding: '18px 24px',
  borderBottom: '1px solid var(--borderColor)',
}

const shellPillStyle: CSSProperties = {
  display: 'inline-flex',
  alignItems: 'center',
  gap: '8px',
  padding: '8px 12px',
  borderRadius: '6px',
  border: '1px solid var(--borderColor)',
  background: 'var(--dockrev-chip)',
  color: 'var(--text)',
  fontSize: '12px',
  fontWeight: 650,
}

const shellBodyStyle: CSSProperties = {
  display: 'grid',
  gridTemplateColumns: 'minmax(0, 1.6fr) minmax(300px, 0.95fr)',
  gap: '18px',
  padding: '20px 24px 28px',
}

const shellPanelStyle: CSSProperties = {
  borderRadius: '6px',
  border: '1px solid var(--borderColor)',
  background: 'var(--panel2)',
  padding: '18px',
}

const shellReleaseBadgeStyle: CSSProperties = {
  display: 'inline-flex',
  minWidth: '28px',
  justifyContent: 'center',
  alignItems: 'center',
  padding: '4px 10px',
  borderRadius: '6px',
  background: 'var(--dockrev-chip)',
  color: 'var(--text)',
  fontSize: '12px',
  fontWeight: 700,
}

function DrawerStoryShell(props: { children: ReactNode }) {
  return (
    <div data-release-drawer-story-shell="true" style={shellStyle}>
      <div style={shellFrameStyle}>
        <header style={shellHeaderStyle}>
          <div style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
            <strong style={{ color: 'var(--text)', fontSize: '20px', letterSpacing: 0 }}>服务更新概览</strong>
            <span style={{ color: 'var(--muted)', fontSize: '13px' }}>
              用稳定的 mock 页面承载右侧 GitHub Releases 抽屉，方便检查间距、滚动和定位反馈。
            </span>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: '10px', flexWrap: 'wrap' }}>
            <span style={shellPillStyle}>全部 62</span>
            <span style={{ ...shellPillStyle, borderColor: 'var(--color-primary)', color: 'var(--text)' }}>可更新 13</span>
            <span style={shellPillStyle}>需确认 0</span>
          </div>
        </header>

        <div style={shellBodyStyle}>
          <section style={{ ...shellPanelStyle, display: 'flex', flexDirection: 'column', gap: '14px' }}>
            {[
              ['ai-codex-vibe-monitor', 'ivanli-cn/codex-vibe-monitor', 'v1.39.5 → v1.40.0', '1 个候选版本'],
              ['axonhub', 'looplj/axonhub', 'latest → v0.9.30', '2 个候选版本'],
              ['searxng', 'searxng/searxng', '2026.4.5 → 2026.4.6', '1 个候选版本'],
            ].map(([serviceName, repoName, versionText, note]) => (
              <div
                key={serviceName}
                style={{
                  display: 'grid',
                  gridTemplateColumns: 'minmax(0, 1.2fr) minmax(0, 1fr) auto',
                  gap: '14px',
                  alignItems: 'center',
                  padding: '14px 16px',
                  borderRadius: '6px',
                  border: '1px solid var(--borderColor)',
                  background: 'var(--dockrev-surface)',
                }}
              >
                <div style={{ display: 'flex', flexDirection: 'column', gap: '4px' }}>
                  <strong style={{ color: 'var(--text)', fontSize: '15px' }}>{serviceName}</strong>
                  <span style={{ color: 'var(--muted)', fontSize: '13px' }}>{repoName}</span>
                </div>
                <div style={{ display: 'flex', flexDirection: 'column', gap: '4px' }}>
                  <span style={{ color: 'var(--text)', fontSize: '14px' }}>{versionText}</span>
                  <span style={{ color: 'var(--muted)', fontSize: '12px' }}>{note}</span>
                </div>
                <span style={shellReleaseBadgeStyle}>Releases</span>
              </div>
            ))}
          </section>

          <aside style={{ ...shellPanelStyle, display: 'flex', flexDirection: 'column', gap: '12px' }}>
            <strong style={{ color: 'var(--text)', fontSize: '15px' }}>这次截图要证明的点</strong>
            {[
              '右侧面板要保留真正的 Drawer 语义：贴边、全高、强层级、明确可关闭。',
              'mock 的 release 数量和内容要像真实发布序列，不是两三条占位数据。',
              '短列表和长列表都要合理：必要时只让抽屉内容区自己滚动，不制造假的“无滚动条”承诺。',
            ].map((line) => (
              <div
                key={line}
                style={{
                  padding: '12px 14px',
                  borderRadius: '6px',
                  background: 'var(--dockrev-surface)',
                  border: '1px solid var(--borderColor)',
                  color: 'var(--text)',
                  fontSize: '13px',
                  lineHeight: 1.55,
                }}
              >
                {line}
              </div>
            ))}
          </aside>
        </div>
      </div>
      {props.children}
    </div>
  )
}

function buildReleaseBody(tagName: string, index: number, compact: boolean): string {
  const set = RELEASE_NOTE_SETS[index % RELEASE_NOTE_SETS.length]
  if (compact) {
    return [
      `${tagName}`,
      '',
      set.summary,
      `- ${set.highlights[0]}`,
      `- ${set.fixes[0]}`,
    ].join('\n')
  }

  return [
    `${tagName}`,
    '',
    set.summary,
    '',
    'Highlights',
    ...set.highlights.map((line) => `- ${line}`),
    '',
    'Fixes',
    ...set.fixes.map((line) => `- ${line}`),
    '',
    'Operational notes',
    ...set.ops.map((line) => `- ${line}`),
  ].join('\n')
}

function buildReleaseItems(tags: string[], compact = false) {
  const startAt = Date.UTC(2026, 3, 7, 10, 40)
  return tags.map((tagName, index) => {
    const publishedAt = new Date(startAt - index * 30 * HOUR_MS)
    const createdAt = new Date(publishedAt.getTime() - 2 * HOUR_MS)
    const displayName =
      index % 5 === 0
        ? `Release ${tagName}`
        : index % 7 === 0
          ? `Stability rollup ${tagName}`
          : tagName

    return {
      id: 70_000 + index,
      tagName,
      name: displayName,
      body: buildReleaseBody(tagName, index, compact),
      htmlUrl: `${STORY_REPO.htmlUrl}/releases/tag/${encodeURIComponent(tagName)}`,
      draft: false,
      prerelease: tagName.includes('-rc.'),
      publishedAt: publishedAt.toISOString(),
      createdAt: createdAt.toISOString(),
    }
  })
}

const scrollableReleaseTags = [
  '1.43.0',
  '1.42.3',
  '1.42.2',
  '1.42.1',
  '1.42.0',
  '1.41.2',
  '1.41.1',
  '1.41.0',
  '1.40.3',
  '1.40.2',
  '1.40.1',
  '1.40.0',
  '1.39.5',
  '1.39.4',
  '1.39.3',
  '1.39.2',
  '1.39.1',
  '1.39.0',
  '1.38.4',
  '1.38.3',
  '1.38.2',
  '1.38.1',
  '1.38.0',
  '1.37.3',
  '1.37.2',
  '1.37.1',
  '1.37.0',
  '1.36.4',
  '1.36.3',
  '1.36.2',
] as const

const compactReleaseTags = ['1.43.0', '1.42.3', '1.42.2', '1.42.1'] as const

const baseDataset: DockrevMockGitHubReleasesDataset = {
  authMode: 'anonymous',
  repo: STORY_REPO,
  items: buildReleaseItems([...scrollableReleaseTags]),
}

const compactDataset: DockrevMockGitHubReleasesDataset = {
  authMode: 'pat',
  repo: STORY_REPO,
  items: buildReleaseItems([...compactReleaseTags], true),
}

const gitHubProviderFixture = (() => {
  const fixture = buildFixture('default')
  fixture.settings.releaseNotes.provider = 'gitHub'
  return fixture
})()

const meta: Meta<typeof GitHubReleaseDrawer> = {
  title: 'Components/GitHubReleaseDrawer',
  tags: ['autodocs'],
  component: GitHubReleaseDrawer,
  decorators: [
    withDockrevMockApi,
    (Story) => (
      <DrawerStoryShell>
        <Story />
      </DrawerStoryShell>
    ),
  ],
  parameters: {
    layout: 'fullscreen',
  },
}

export default meta

type Story = StoryObj<typeof GitHubReleaseDrawer>

const releaseDrawerLocatedDataset = {
  'svc-release-drawer': {
    ...baseDataset,
    locateByVersion: {
      '1.39.5': {
        status: 'found',
        matchedTag: '1.39.5',
        indexWithinWindow: 12,
        absoluteIndex: 12,
      },
    },
  },
}

async function openInfoTooltip(): Promise<HTMLElement> {
  const trigger = document.querySelector('[data-release-drawer-info-trigger="true"]')
  if (!(trigger instanceof HTMLElement)) throw new Error('expected release drawer info trigger')
  trigger.dispatchEvent(new PointerEvent('pointermove', { bubbles: true }))
  trigger.dispatchEvent(new MouseEvent('mouseenter', { bubbles: true }))
  trigger.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }))

  for (let index = 0; index < 6; index += 1) {
    const tooltip = document.querySelector('[data-release-drawer-info-tooltip="true"]')
    if (tooltip instanceof HTMLElement) return tooltip
    await new Promise((resolve) => setTimeout(resolve, 60))
  }

  throw new Error('expected release drawer info tooltip')
}

function assertVisibleReleaseRowsDoNotOverlap() {
  const scrollRegion = document.querySelector('.releaseDrawerScrollViewport')
  if (!(scrollRegion instanceof HTMLElement)) throw new Error('expected release drawer scroll region')
  const scrollRect = scrollRegion.getBoundingClientRect()
  const rows = Array.from(document.querySelectorAll('[data-release-tag]'))
    .filter((node): node is HTMLElement => node instanceof HTMLElement)
    .map((node) => ({
      tag: node.dataset.releaseTag ?? '(unknown)',
      rect: node.getBoundingClientRect(),
    }))
    .filter((row) => row.rect.bottom > scrollRect.top && row.rect.top < scrollRect.bottom)

  rows.slice(1).forEach((row, index) => {
    const prev = rows[index]
    if (row.rect.top < prev.rect.bottom - 1) {
      throw new Error(`expected visible release rows not to overlap: ${prev.tag} -> ${row.tag}`)
    }
  })
}

function readReleaseDrawerLayoutSnapshot() {
  const header = document.querySelector('.releaseDrawerHeader')
  const banner = document.querySelector('[data-release-drawer-banner]')
  const scrollRegion = document.querySelector('.releaseDrawerScrollViewport')
  if (!(header instanceof HTMLElement)) throw new Error('expected release drawer header')
  if (!(banner instanceof HTMLElement)) throw new Error('expected release drawer banner')
  if (!(scrollRegion instanceof HTMLElement)) throw new Error('expected release drawer scroll region')

  const headerRect = header.getBoundingClientRect()
  const bannerRect = banner.getBoundingClientRect()
  const scrollRect = scrollRegion.getBoundingClientRect()

  return {
    headerBottom: headerRect.bottom,
    bannerTop: bannerRect.top,
    scrollTop: scrollRect.top,
  }
}

function assertTooltipDoesNotChangeDocumentFlow(before: ReturnType<typeof readReleaseDrawerLayoutSnapshot>) {
  const after = readReleaseDrawerLayoutSnapshot()
  const changed =
    Math.abs(after.headerBottom - before.headerBottom) > 1 ||
    Math.abs(after.bannerTop - before.bannerTop) > 1 ||
    Math.abs(after.scrollTop - before.scrollTop) > 1

  if (changed) {
    throw new Error('expected info tooltip to overlay without changing the header/document flow layout')
  }
}

export const OctoRillSmartDefault: Story = {
  args: {
    open: true,
    serviceId: 'svc-release-drawer',
    onOpenChange: () => {},
  },
  parameters: {
    dockrevApiScenario: 'default',
    dockrevGitHubReleasesByServiceId: {
      'svc-release-drawer': baseDataset,
    },
  },
  play: async () => {
    await new Promise((resolve) => setTimeout(resolve, 360))
    const drawer = document.querySelector('[data-release-drawer="true"]')
    if (!(drawer instanceof HTMLElement)) throw new Error('expected release drawer content to render')
    if (drawer.textContent?.includes('查看该服务对应仓库的发布记录')) {
      throw new Error('expected the release drawer to omit implementation-oriented header copy')
    }
    const controls = document.querySelector('.releaseDrawerHeaderControls')
    const meta = document.querySelector('.releaseDrawerHeaderMeta')
    const tabs = document.querySelector('.releaseDrawerViewTabs')
    if (!(controls instanceof HTMLElement) || !(meta instanceof HTMLElement) || !(tabs instanceof HTMLElement)) {
      throw new Error('expected repository metadata and release-note views to share the header controls row')
    }
    const controlsStyle = getComputedStyle(controls)
    if (controlsStyle.display !== 'grid' || controlsStyle.gridTemplateColumns.split(' ').length !== 2) {
      throw new Error('expected desktop header controls to use a two-column end-aligned layout')
    }
    const controlsRect = controls.getBoundingClientRect()
    const tabsRect = tabs.getBoundingClientRect()
    if (Math.abs(meta.getBoundingClientRect().top - tabsRect.top) > 2 || Math.abs(tabsRect.right - controlsRect.right) > 2) {
      throw new Error('expected repository metadata and view controls to align on one row at opposite ends')
    }
    if (!drawer.textContent?.includes('润色摘要')) {
      throw new Error('expected smart release notes to be visible by default')
    }
    const activeView = document.querySelector('.releaseDrawerViewTabActive')
    if (!activeView?.textContent?.includes('润色')) {
      throw new Error('expected smart view tab to be active by default')
    }
  },
}

export const GitHubOriginalOnly: Story = {
  args: {
    open: true,
    serviceId: 'svc-release-drawer-github',
    onOpenChange: () => {},
  },
  parameters: {
    dockrevApiScenario: 'default',
    dockrevInitialFixture: gitHubProviderFixture,
    dockrevGitHubReleasesByServiceId: {
      'svc-release-drawer-github': baseDataset,
    },
  },
  play: async () => {
    await new Promise((resolve) => setTimeout(resolve, 360))
    const drawer = document.querySelector('[data-release-drawer="true"]')
    if (!(drawer instanceof HTMLElement)) throw new Error('expected release drawer content to render')
    if (drawer.textContent?.includes('润色摘要') || drawer.textContent?.includes('翻译：')) {
      throw new Error('expected GitHub provider to keep only the original release body')
    }
    if (document.querySelectorAll('.releaseDrawerViewTab').length !== 0) {
      throw new Error('expected GitHub provider to hide translated/smart view tabs')
    }
    const chips = Array.from(document.querySelectorAll('.releaseDrawerChip')).map((node) => node.textContent?.trim() ?? '')
    if (!chips.includes('GitHub Releases')) {
      throw new Error('expected GitHub provider source chip')
    }
  },
}

export const AnonymousLocated: Story = {
  args: {
    open: true,
    serviceId: 'svc-release-drawer',
    version: '1.39.5',
    onOpenChange: () => {},
  },
  parameters: {
    dockrevApiScenario: 'default',
    dockrevGitHubReleasesByServiceId: releaseDrawerLocatedDataset,
  },
  play: async () => {
    await new Promise((resolve) => setTimeout(resolve, 360))
    const drawer = document.querySelector('[data-release-drawer="true"]')
    if (!(drawer instanceof HTMLElement)) throw new Error('expected release drawer content to render')
    const closeButton = document.querySelector('[data-release-drawer-close="true"]')
    if (!(closeButton instanceof HTMLButtonElement)) throw new Error('expected drawer to expose an explicit close button')
    if (drawer.scrollHeight <= drawer.clientHeight) {
      throw new Error('expected scrollable story to overflow inside the drawer content region')
    }

    const target = document.querySelector('[data-release-tag="1.39.5"]')
    if (!target) throw new Error('expected targeted release row to exist')
    const banner = document.querySelector('[data-release-drawer-banner="success"]')
    if (!banner) throw new Error('expected locate success banner')
    if (!drawer.textContent?.includes('润色摘要')) {
      throw new Error('expected smart release notes to be visible by default')
    }
    const translatedTab = Array.from(document.querySelectorAll<HTMLButtonElement>('.releaseDrawerViewTab')).find(
      (button) => button.textContent?.trim() === '翻译',
    )
    if (!translatedTab) throw new Error('expected translated view tab')
    translatedTab.click()
    await new Promise((resolve) => setTimeout(resolve, 60))
    if (!drawer.textContent?.includes('翻译：')) {
      throw new Error('expected translated release notes after switching view')
    }

    const chips = Array.from(document.querySelectorAll('.releaseDrawerChip')).map((node) => node.textContent?.trim() ?? '')
    if (chips.includes('定位 1.39.5')) {
      throw new Error('expected target version to move out of always-visible header chips')
    }

    const layoutBeforeTooltip = readReleaseDrawerLayoutSnapshot()
    await new Promise((resolve) => setTimeout(resolve, 160))
    const tooltip = await openInfoTooltip()
    await new Promise((resolve) => setTimeout(resolve, 160))
    if (!tooltip.textContent?.includes('数据来源') || !tooltip.textContent?.includes('OctoRill')) {
      throw new Error('expected tooltip to expose release notes source')
    }
    if (!tooltip.textContent?.includes('默认视图') || !tooltip.textContent?.includes('润色')) {
      throw new Error('expected tooltip to expose default release notes view')
    }
    if (!tooltip.textContent?.includes('定位版本') || !tooltip.textContent?.includes('1.39.5')) {
      throw new Error('expected tooltip to expose target version')
    }

    const scrollRegion = document.querySelector('.releaseDrawerScrollViewport')
    if (!(scrollRegion instanceof HTMLElement)) throw new Error('expected release drawer scroll region')
    if (!(banner instanceof HTMLElement)) throw new Error('expected release drawer banner')
    const tooltipRect = tooltip.getBoundingClientRect()
    const scrollRect = scrollRegion.getBoundingClientRect()
    const bannerRect = banner.getBoundingClientRect()
    if (tooltipRect.bottom > bannerRect.top - 2) {
      throw new Error('expected info tooltip to stay above the locate banner')
    }
    if (tooltipRect.bottom > scrollRect.top - 8) {
      throw new Error('expected info tooltip to stay inside the header area without overlapping release rows')
    }

    assertTooltipDoesNotChangeDocumentFlow(layoutBeforeTooltip)
    assertVisibleReleaseRowsDoNotOverlap()
  },
}

export const PatAuthenticatedShortList: Story = {
  args: {
    open: true,
    serviceId: 'svc-release-drawer-pat',
    onOpenChange: () => {},
  },
  parameters: {
    dockrevApiScenario: 'default',
    dockrevGitHubReleasesByServiceId: {
      'svc-release-drawer-pat': compactDataset,
    },
  },
  play: async () => {
    await new Promise((resolve) => setTimeout(resolve, 220))
    const drawer = document.querySelector('[data-release-drawer="true"]')
    if (!(drawer instanceof HTMLElement)) throw new Error('expected release drawer content to render')
    const closeButton = document.querySelector('[data-release-drawer-close="true"]')
    if (!(closeButton instanceof HTMLButtonElement)) throw new Error('expected drawer to expose an explicit close button')

    const chips = Array.from(document.querySelectorAll('.releaseDrawerChip')).map((node) => node.textContent?.trim() ?? '')
    if (!chips.includes('OctoRill')) throw new Error('expected OctoRill source chip')

    const layoutBeforeTooltip = readReleaseDrawerLayoutSnapshot()
    await new Promise((resolve) => setTimeout(resolve, 160))
    const tooltip = await openInfoTooltip()
    await new Promise((resolve) => setTimeout(resolve, 160))
    if (!tooltip.textContent?.includes('数据来源') || !tooltip.textContent?.includes('OctoRill')) {
      throw new Error('expected info tooltip to expose release notes source')
    }

    const scrollRegion = document.querySelector('.releaseDrawerScrollViewport')
    if (!(scrollRegion instanceof HTMLElement)) throw new Error('expected release drawer scroll region')
    const tooltipRect = tooltip.getBoundingClientRect()
    const scrollRect = scrollRegion.getBoundingClientRect()
    if (tooltipRect.bottom > scrollRect.top - 8) {
      throw new Error('expected info tooltip to stay above the release rows in compact mode')
    }

    assertTooltipDoesNotChangeDocumentFlow(layoutBeforeTooltip)
    assertVisibleReleaseRowsDoNotOverlap()
  },
}

export const PermissionDenied: Story = {
  args: {
    open: true,
    serviceId: 'svc-release-drawer-denied',
    version: '1.39.5',
    onOpenChange: () => {},
  },
  parameters: {
    dockrevApiScenario: 'default',
    dockrevGitHubReleasesByServiceId: {
      'svc-release-drawer-denied': {
        authMode: 'anonymous',
        repo: {
          fullName: 'ivanli-cn/private-monitor',
          htmlUrl: 'https://github.com/ivanli-cn/private-monitor',
        },
        listStatus: 'permissionDenied',
      },
    },
  },
  play: async () => {
    await new Promise((resolve) => setTimeout(resolve, 180))
    const state = document.querySelector('[data-release-drawer-state="upstreamError"]')
    if (!state) throw new Error('expected upstream error state to render')
  },
}

export const OutsideWindow: Story = {
  args: {
    open: true,
    serviceId: 'svc-release-drawer-window',
    version: '1.39.5',
    onOpenChange: () => {},
  },
  parameters: {
    dockrevApiScenario: 'default',
    dockrevGitHubReleasesByServiceId: {
      'svc-release-drawer-window': {
        ...baseDataset,
        items: buildReleaseItems([
          ...scrollableReleaseTags,
          '1.36.1',
          '1.36.0',
          '1.35.4',
          '1.35.3',
          '1.35.2',
          '1.35.1',
          '1.35.0',
          '1.34.4',
          '1.34.3',
          '1.34.2',
          '1.34.1',
          '1.34.0',
          '1.33.4',
          '1.33.3',
          '1.33.2',
          '1.33.1',
          '1.33.0',
          '1.32.4',
          '1.32.3',
          '1.32.2',
          '1.32.1',
          '1.32.0',
          '1.31.4',
          '1.31.3',
          '1.31.2',
          '1.31.1',
          '1.31.0',
          '1.30.4',
          '1.30.3',
          '1.30.2',
        ]),
        locateByVersion: {
          '1.39.5': {
            status: 'outsideWindow',
            matchedTag: '1.39.5',
          },
        },
      },
    },
  },
}
