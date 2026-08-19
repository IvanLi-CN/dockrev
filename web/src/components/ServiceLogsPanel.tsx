import { useEffect, useLayoutEffect, useMemo, useState } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import { Input, Button, OverlayScrollArea, Pill, SearchIcon } from '../ui'
import {
  SERVICE_LOG_BUFFER_LIMIT,
  useServiceLogsState,
  type ServiceLogRecord,
} from '../pages/useServiceLogsState'

const LOCAL_TZ = Intl.DateTimeFormat().resolvedOptions().timeZone || 'Local'
type LogTimeZone = 'local' | 'utc'
type LogViewMode = 'human' | 'raw'

function pad2(n: number): string {
  return String(n).padStart(2, '0')
}

function pad3(n: number): string {
  return String(n).padStart(3, '0')
}

function formatLogStamp(
  ts: string,
  tz: LogTimeZone,
): { date: string; time: string; title: string; isValid: boolean } {
  const value = (ts ?? '').trim()
  if (!value) {
    return {
      date: '-',
      time: '-',
      title: '-',
      isValid: false,
    }
  }
  const date = new Date(value)
  if (Number.isNaN(date.valueOf())) {
    return {
      date: value,
      time: '',
      title: value,
      isValid: false,
    }
  }
  const year = tz === 'utc' ? date.getUTCFullYear() : date.getFullYear()
  const month = tz === 'utc' ? date.getUTCMonth() + 1 : date.getMonth() + 1
  const day = tz === 'utc' ? date.getUTCDate() : date.getDate()
  const hours = tz === 'utc' ? date.getUTCHours() : date.getHours()
  const minutes = tz === 'utc' ? date.getUTCMinutes() : date.getMinutes()
  const seconds = tz === 'utc' ? date.getUTCSeconds() : date.getSeconds()
  const milliseconds = tz === 'utc' ? date.getUTCMilliseconds() : date.getMilliseconds()
  return {
    date: `${year}-${pad2(month)}-${pad2(day)}`,
    time: `${pad2(hours)}:${pad2(minutes)}:${pad2(seconds)}.${pad3(milliseconds)}`,
    title: `${LOCAL_TZ}: ${date.toLocaleString()} · UTC: ${date.toISOString()}`,
    isValid: true,
  }
}

function formatLogLevel(level: string): string {
  const value = (level ?? '').trim().toLowerCase()
  if (!value || value === 'unknown') return 'LOG'
  if (value === 'info') return 'INFO'
  if (value === 'warn' || value === 'warning') return 'WARN'
  if (value === 'error' || value === 'err') return 'ERR'
  if (value === 'debug') return 'DBG'
  if (value === 'trace') return 'TRC'
  return value.slice(0, 4).toUpperCase()
}

function formatMetaValue(value: unknown): string {
  if (value == null) return ''
  if (typeof value === 'string') return value
  if (typeof value === 'number' || typeof value === 'boolean') return String(value)
  try {
    return JSON.stringify(value)
  } catch {
    return String(value)
  }
}

function metadataEntries(record: ServiceLogRecord): Array<{ key: string; value: string }> {
  const attributes = record.meta?.attributes ?? {}
  const preferredKeys = (record.meta?.highlights ?? []).filter((key) => key in attributes)
  const keys = preferredKeys.length > 0 ? preferredKeys : Object.keys(attributes).slice(0, 5)
  return keys
    .map((key) => ({ key, value: formatMetaValue(attributes[key]) }))
    .filter((entry) => entry.value.length > 0)
    .slice(0, 6)
}

export function ServiceLogsPanel(props: { serviceId: string }) {
  const { error, filteredRecords, loading, query, records, resetNonce, setQuery } = useServiceLogsState(
    props.serviceId,
  )
  const [scrollViewport, setScrollViewport] = useState<HTMLElement | null>(null)
  const [follow, setFollow] = useState(true)
  const [isAtBottom, setIsAtBottom] = useState(true)
  const [logTz, setLogTz] = useState<LogTimeZone>('local')
  const [logView, setLogView] = useState<LogViewMode>('human')
  const [wrapLines, setWrapLines] = useState(false)

  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: filteredRecords.length,
    getScrollElement: () => scrollViewport,
    estimateSize: () => 42,
    overscan: 12,
    getItemKey: (index) => filteredRecords[index]?.id ?? index,
    measureElement: (element) => element.getBoundingClientRect().height,
  })

  useEffect(() => {
    if (!scrollViewport) return
    const element = scrollViewport
    const onScroll = () => {
      const nearBottom = element.scrollHeight - element.scrollTop - element.clientHeight < 48
      setIsAtBottom(nearBottom)
      if (!nearBottom) setFollow(false)
      else if (!query.trim()) setFollow(true)
    }
    onScroll()
    element.addEventListener('scroll', onScroll)
    return () => element.removeEventListener('scroll', onScroll)
  }, [query, scrollViewport])

  useEffect(() => {
    if (query.trim()) setFollow(false)
    else if (isAtBottom) setFollow(true)
  }, [isAtBottom, query])

  const hasQuery = query.trim().length > 0
  const latestRecordId = filteredRecords.at(-1)?.id

  useLayoutEffect(() => {
    virtualizer.measure()
  }, [logView, resetNonce, virtualizer, wrapLines])

  useLayoutEffect(() => {
    if (!follow || hasQuery) return
    if (filteredRecords.length === 0) return
    const targetIndex = filteredRecords.length - 1
    virtualizer.scrollToIndex(targetIndex, { align: 'end' })

    let alignTailFrame: number | undefined
    const measureTailFrame = window.requestAnimationFrame(() => {
      alignTailFrame = window.requestAnimationFrame(() => {
        const tail = scrollViewport?.querySelector<HTMLElement>(
          `.serviceLogRow[data-index="${targetIndex}"]`,
        )
        if (tail) virtualizer.measureElement(tail)
        virtualizer.scrollToIndex(targetIndex, { align: 'end' })
      })
    })

    return () => {
      window.cancelAnimationFrame(measureTailFrame)
      if (alignTailFrame != null) window.cancelAnimationFrame(alignTailFrame)
    }
  }, [filteredRecords.length, follow, hasQuery, latestRecordId, logView, resetNonce, scrollViewport, virtualizer, wrapLines])

  const items = virtualizer.getVirtualItems()
  const offsetTop = items[0]?.start ?? 0
  const showJump = !follow && records.length > 0
  const renderedCount = Math.min(items.length, filteredRecords.length)
  const errorCount = useMemo(
    () => filteredRecords.reduce((count, record) => (record.level === 'error' ? count + 1 : count), 0),
    [filteredRecords],
  )
  const warnCount = useMemo(
    () => filteredRecords.reduce((count, record) => (record.level === 'warn' ? count + 1 : count), 0),
    [filteredRecords],
  )
  const resultSummary = useMemo(() => {
    if (!hasQuery) return `${records.length} 行`
    return `${filteredRecords.length} / ${records.length} 行匹配`
  }, [filteredRecords.length, hasQuery, records.length])
  const emptyState = loading
    ? {
        title: '连接日志流…',
        detail: '正在抓取最近缓冲并建立实时续流。',
      }
    : filteredRecords.length === 0
      ? records.length === 0
        ? {
            title: '当前没有日志输出',
            detail: '服务恢复输出后会自动继续接流。',
          }
        : {
            title: '没有匹配结果',
            detail: '搜索只覆盖当前内存缓冲；清空查询后可回到完整时间流。',
          }
      : null

  return (
    <div className="serviceLogsPanel card" data-service-detail-section-card="logs-panel">
      <div className="serviceLogsToolbar">
        <div className="serviceLogsToolbarLeft">
          <div className="serviceLogsHeading">
            <div className="serviceLogsTitleRow">
              <div className="title">实时日志</div>
              <span className={follow && !hasQuery ? 'serviceLogsLiveDot active' : 'serviceLogsLiveDot'} aria-hidden="true" />
            </div>
          </div>
        </div>
        <label className="serviceLogsSearch">
          <SearchIcon className="serviceLogsSearchIcon" />
          <Input
            aria-label="搜索日志"
            className="input serviceLogsSearchInput"
            onChange={(event) => setQuery(event.target.value)}
            placeholder="搜索当前缓冲"
            value={query}
          />
        </label>
      </div>

      <div className="serviceLogsStatusRow">
        <div className="serviceLogsStatusPills">
          <Pill tone={hasQuery ? 'warn' : 'muted'}>{resultSummary}</Pill>
          <Pill tone={hasQuery ? 'warn' : follow ? 'info' : 'muted'}>
            {hasQuery ? '筛选中' : follow ? '跟随最新' : '暂停跟随'}
          </Pill>
          {errorCount > 0 ? <Pill tone="bad">{`ERR ${errorCount}`}</Pill> : null}
          {warnCount > 0 ? <Pill tone="warn">{`WARN ${warnCount}`}</Pill> : null}
          <Pill tone="muted">{`缓冲 ${records.length}/${SERVICE_LOG_BUFFER_LIMIT}`}</Pill>
          <Pill tone="muted">{`虚拟 ${renderedCount}/${filteredRecords.length}`}</Pill>
        </div>
        <div className="serviceLogsStatusSide">
          <div className="serviceLogsControls">
            <div className="serviceLogsToggleGroup" role="group" aria-label="日志显示模式">
              <Button
                ariaPressed={logView === 'human'}
                className="serviceLogsMiniToggle"
                onClick={() => setLogView('human')}
                title="显示解析后的消息与元数据。"
                variant={logView === 'human' ? 'primary' : 'ghost'}
              >
                Human
              </Button>
              <Button
                ariaPressed={logView === 'raw'}
                className="serviceLogsMiniToggle"
                onClick={() => setLogView('raw')}
                title="显示容器原始输出。"
                variant={logView === 'raw' ? 'primary' : 'ghost'}
              >
                Raw
              </Button>
            </div>
            <div className="serviceLogsToggleGroup" role="group" aria-label="日志时间时区">
              <Button
                ariaPressed={logTz === 'local'}
                className="serviceLogsMiniToggle"
                onClick={() => setLogTz('local')}
                title={`浏览器时区：${LOCAL_TZ}`}
                variant={logTz === 'local' ? 'primary' : 'ghost'}
              >
                本地
              </Button>
              <Button
                ariaPressed={logTz === 'utc'}
                className="serviceLogsMiniToggle"
                onClick={() => setLogTz('utc')}
                title="后端时间戳为 RFC3339（UTC）"
                variant={logTz === 'utc' ? 'primary' : 'ghost'}
              >
                UTC
              </Button>
            </div>
            <Button
              ariaPressed={wrapLines}
              className="serviceLogsWrapToggle"
              onClick={() => setWrapLines((value) => !value)}
              title={wrapLines ? '关闭自动换行，按原始行滚动查看。' : '开启自动换行，长行会在当前视口内折行。'}
              variant={wrapLines ? 'primary' : 'ghost'}
            >
              {wrapLines ? '自动换行 开' : '自动换行 关'}
            </Button>
          </div>
        </div>
      </div>

      {error ? <div className="error serviceLogsError">{error}</div> : null}

      <div
        className="serviceLogsTerminal"
        data-service-logs-total-count={filteredRecords.length}
        data-service-logs-visible-count={renderedCount}
        data-service-logs-virtualized="true"
        data-service-logs-view={logView}
        data-service-logs-wrap={wrapLines ? 'on' : 'off'}
        data-wrap={wrapLines ? 'on' : 'off'}
      >
        <div className="serviceLogsTerminalHead" aria-hidden="true">
          <span>时间</span>
          <span>等级</span>
          <span>输出</span>
        </div>
        <div className="serviceLogsTerminalBody">
          <OverlayScrollArea
            className="serviceLogsViewport"
            defer={false}
            onViewportReady={setScrollViewport}
            viewportLabel="服务实时日志"
          >
            {emptyState ? (
              <div className="serviceLogsEmptyState">
                <div className="serviceLogsEmptyTitle">{emptyState.title}</div>
                <div className="muted serviceLogsEmptyDetail">{emptyState.detail}</div>
              </div>
            ) : null}
            {filteredRecords.length > 0 ? (
              <div
                style={{
                  height: `${virtualizer.getTotalSize()}px`,
                  position: 'relative',
                  width: '100%',
                }}
              >
                <div
                  style={{
                    position: 'absolute',
                    top: 0,
                    left: 0,
                    width: '100%',
                    transform: `translateY(${offsetTop}px)`,
                  }}
                >
                  {items.map((item) => {
                    const record = filteredRecords[item.index]
                    if (!record) return null
                    const stamp = formatLogStamp(record.ts, logTz)
                    const previousRecord = filteredRecords[item.index - 1]
                    const previousStamp = previousRecord ? formatLogStamp(previousRecord.ts, logTz) : null
                    const showDateDivider = stamp.isValid && (!previousStamp?.isValid || previousStamp.date !== stamp.date)
                    const metaEntries = logView === 'human' ? metadataEntries(record) : []
                    return (
                      <div
                        className="serviceLogRow"
                        data-following={follow ? 'true' : 'false'}
                        data-format={record.meta?.format ?? 'unknown'}
                        data-index={item.index}
                        data-inline-level={record.inlineLevel ? 'true' : 'false'}
                        data-level={record.level}
                        data-log-date={stamp.isValid ? stamp.date : undefined}
                        data-multiline={record.multiline ? 'true' : 'false'}
                        data-date-divider={showDateDivider ? 'true' : 'false'}
                        data-view={logView}
                        data-wrap={wrapLines ? 'true' : 'false'}
                        key={record.id}
                        ref={virtualizer.measureElement}
                      >
                        {showDateDivider ? <div className="serviceLogDateDivider">{stamp.date}</div> : null}
                        <span className="mono serviceLogTs" data-valid={stamp.isValid ? 'true' : 'false'} title={stamp.title}>
                          {stamp.time ? <span className="serviceLogTsTime">{stamp.time}</span> : null}
                          <span className="serviceLogTsDate">{stamp.date}</span>
                        </span>
                        <span
                          className={`mono logLvl serviceLogLevel logLvl-${record.level}${record.inlineLevel ? ' serviceLogLevelInline' : ''}`}
                          data-level={record.level}
                          title={
                            record.inlineLevel
                              ? `等级已包含在输出中：${formatLogLevel(record.level)}`
                              : record.meta?.level
                                ? `应用日志等级：${formatLogLevel(record.level)}`
                                : `等级：${formatLogLevel(record.level)}（基于 ANSI 颜色与关键词推断）`
                          }
                        >
                          {record.inlineLevel ? '' : formatLogLevel(record.level)}
                        </span>
                        <span className="serviceLogMsg">
                          {logView === 'human' ? (
                            <>
                              <span className="serviceLogHumanMsg">{record.message}</span>
                              {record.meta?.format && record.meta.format !== 'text' ? (
                                <span className="serviceLogMetaFormat">{record.meta.format.toUpperCase()}</span>
                              ) : null}
                              {metaEntries.length > 0 ? (
                                <span className="serviceLogMetaChips" aria-label="日志元数据">
                                  {metaEntries.map((entry) => (
                                    <span className="serviceLogMetaChip" key={`${record.id}-${entry.key}`}>
                                      <span className="serviceLogMetaKey">{entry.key}</span>
                                      <span className="serviceLogMetaValue">{entry.value}</span>
                                    </span>
                                  ))}
                                </span>
                              ) : null}
                            </>
                          ) : (
                            record.segments.map((segment, index) => (
                              <span key={`${record.id}-${index}`} style={segment.style}>
                                {segment.text}
                              </span>
                            ))
                          )}
                        </span>
                      </div>
                    )
                  })}
                </div>
              </div>
            ) : null}
          </OverlayScrollArea>

          {showJump ? (
            <div className="serviceLogsJumpWrap">
              <Button
                variant="primary"
                onClick={() => {
                  if (filteredRecords.length > 0) {
                    virtualizer.scrollToIndex(filteredRecords.length - 1, { align: 'end' })
                  }
                  setFollow(true)
                }}
              >
                {hasQuery ? '跳到结果底部' : '跳到最新'}
              </Button>
            </div>
          ) : null}
        </div>
      </div>
    </div>
  )
}
