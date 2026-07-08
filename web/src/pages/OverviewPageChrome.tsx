import type { ReactNode } from 'react'
import { Clock3, Cpu, Download, MemoryStick, Search, Upload } from 'lucide-react'
import { Input } from '../ui'

export type OverviewMetricsSummary = {
  activeCount: number
  cpu: number | null
  memory: number | null
  rx: number | null
  tx: number | null
}

function formatPercent(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return '-'
  if (value < 10) return `${value.toFixed(1)}%`
  return `${value.toFixed(0)}%`
}

function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null || !Number.isFinite(bytes)) return '-'
  const units = ['B', 'kB', 'MB', 'GB', 'TB']
  let value = bytes
  let idx = 0
  while (value >= 1024 && idx < units.length - 1) {
    value /= 1024
    idx += 1
  }
  const digits = idx === 0 || value >= 100 ? 0 : value >= 10 ? 1 : 2
  return `${value.toFixed(digits)} ${units[idx]}`
}

function formatRate(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return '-'
  if (value < 1) return '0 B/s'
  return `${formatBytes(value)}/s`
}

function formatClock(date: Date): string {
  const pad2 = (value: number) => String(value).padStart(2, '0')
  return `${pad2(date.getHours())}:${pad2(date.getMinutes())}:${pad2(date.getSeconds())}`
}

function formatGmtOffset(date: Date): string {
  const offsetMinutes = -date.getTimezoneOffset()
  const sign = offsetMinutes >= 0 ? '+' : '-'
  const abs = Math.abs(offsetMinutes)
  const hours = Math.trunc(abs / 60)
  const minutes = abs % 60
  return minutes === 0
    ? `GMT${sign}${hours}`
    : `GMT${sign}${hours}:${String(minutes).padStart(2, '0')}`
}

function DashboardMetric(props: {
  icon: ReactNode
  value: string
  label: string
}) {
  return (
    <div className="homepageTopMetric">
      <span className="homepageTopMetricIcon" aria-hidden="true">
        {props.icon}
      </span>
      <span className="homepageTopMetricValue">{props.value}</span>
      <span className="homepageTopMetricLabel">{props.label}</span>
    </div>
  )
}

export function HomepageClockBlock(props: {
  className?: string
  clockLabel: string
  now: Date
}) {
  return (
    <div
      className={props.className ? `homepageClock ${props.className}` : 'homepageClock'}
      aria-label={props.clockLabel}
    >
      <Clock3 className="homepageClockIcon" aria-hidden="true" />
      <span>{formatClock(props.now)}</span>
      <span className="homepageClockZone">{formatGmtOffset(props.now)}</span>
    </div>
  )
}

export function HomepageResourceMetrics(props: {
  className?: string
  metricsLabel: string
  summary: OverviewMetricsSummary
}) {
  return (
    <div
      className={props.className ? `homepageSystemMetrics ${props.className}` : 'homepageSystemMetrics'}
      aria-label={props.metricsLabel}
    >
      <DashboardMetric icon={<Cpu />} value={formatPercent(props.summary.cpu)} label="CPU" />
      <DashboardMetric
        icon={<MemoryStick />}
        value={formatBytes(props.summary.memory)}
        label="MEM"
      />
      <DashboardMetric icon={<Download />} value={formatRate(props.summary.rx)} label="RX" />
      <DashboardMetric icon={<Upload />} value={formatRate(props.summary.tx)} label="TX" />
    </div>
  )
}

export function HomepageSearchForm(props: {
  searchDraft: string
  autoFocus?: boolean
  onSearchDraftChange: (value: string) => void
  onApplySearch: () => void
  onEscape?: () => void
}) {
  return (
    <form
      className="homepageOverviewSearchForm"
      onSubmit={(event) => {
        event.preventDefault()
        props.onApplySearch()
      }}
    >
      <div className="homepageOverviewSearchShell">
        <Input
          aria-label="搜索服务入口"
          autoFocus={props.autoFocus}
          className="input homepageOverviewSearchInput"
          name="overview-search"
          onKeyDown={(event) => {
            if (event.key === 'Escape') props.onEscape?.()
          }}
          onChange={(event) => props.onSearchDraftChange(event.target.value)}
          placeholder="搜索服务入口..."
          type="search"
          value={props.searchDraft}
        />
      </div>
    </form>
  )
}

export function HomepageTopStrip(props: {
  className?: string
  metricsLabel: string
  clockLabel: string
  summary: OverviewMetricsSummary
  now: Date
  showClock?: boolean
}) {
  const className = props.className ? `homepageTopStrip ${props.className}` : 'homepageTopStrip'

  return (
    <div className={className}>
      <HomepageResourceMetrics metricsLabel={props.metricsLabel} summary={props.summary} />
      {props.showClock === false ? null : (
        <HomepageClockBlock clockLabel={props.clockLabel} now={props.now} />
      )}
    </div>
  )
}

export function HomepageHeaderContent(props: {
  metricsLabel: string
  summary: OverviewMetricsSummary
  searchDraft: string
  searchOpen: boolean
  onSearchDraftChange: (value: string) => void
  onApplySearch: () => void
  onToggleSearch: () => void
  onCloseSearch: () => void
}) {
  return (
    <div className="homepageHeaderContent">
      <HomepageResourceMetrics
        className="homepageHeaderMetrics"
        metricsLabel={props.metricsLabel}
        summary={props.summary}
      />
      <div className="homepageHeaderSearch">
        <div className="homepageHeaderSearchDesktop">
          <HomepageSearchForm
            searchDraft={props.searchDraft}
            onSearchDraftChange={props.onSearchDraftChange}
            onApplySearch={props.onApplySearch}
          />
        </div>
        <button
          type="button"
          className="homepageHeaderSearchToggle"
          aria-label={props.searchOpen ? '关闭搜索' : '打开搜索'}
          aria-expanded={props.searchOpen}
          onClick={props.onToggleSearch}
        >
          <Search size={19} strokeWidth={2.3} aria-hidden="true" />
        </button>
        {props.searchOpen ? (
          <div className="homepageHeaderSearchPopover">
            <HomepageSearchForm
              autoFocus
              searchDraft={props.searchDraft}
              onSearchDraftChange={props.onSearchDraftChange}
              onApplySearch={props.onApplySearch}
              onEscape={props.onCloseSearch}
            />
          </div>
        ) : null}
      </div>
    </div>
  )
}

export function HomepageSidebarClock(props: { now: Date }) {
  return (
    <div className="homepageSidebarClockPanel">
      <div className="homepageSidebarClockLabel">当前时间</div>
      <HomepageClockBlock
        className="homepageSidebarClock"
        clockLabel="侧边栏当前时间"
        now={props.now}
      />
    </div>
  )
}

export function CardMetric(props: { value: string; label: string }) {
  return (
    <span className="homepageServiceMetric">
      <span className="homepageServiceMetricValue">{props.value}</span>
      <span className="homepageServiceMetricLabel">{props.label}</span>
    </span>
  )
}
