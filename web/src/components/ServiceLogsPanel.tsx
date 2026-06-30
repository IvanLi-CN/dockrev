import { useEffect, useMemo, useRef, useState } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import { Input, Button, Pill, SearchIcon } from '../ui'
import { SERVICE_LOG_BUFFER_LIMIT, useServiceLogsState } from '../pages/useServiceLogsState'

const LOCAL_TZ = Intl.DateTimeFormat().resolvedOptions().timeZone || 'Local'
type LogTimeZone = 'local' | 'utc'

function pad2(n: number): string {
  return String(n).padStart(2, '0')
}

function pad3(n: number): string {
  return String(n).padStart(3, '0')
}

function formatLogStamp(
  ts: string,
  tz: LogTimeZone,
): { date: string; time: string; title: string } {
  const value = (ts ?? '').trim()
  if (!value) {
    return {
      date: '-',
      time: '-',
      title: '-',
    }
  }
  const date = new Date(value)
  if (Number.isNaN(date.valueOf())) {
    return {
      date: value,
      time: '',
      title: value,
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

export function ServiceLogsPanel(props: { serviceId: string }) {
  const { error, filteredRecords, loading, query, records, resetNonce, setQuery } = useServiceLogsState(
    props.serviceId,
  )
  const scrollRef = useRef<HTMLDivElement | null>(null)
  const [follow, setFollow] = useState(true)
  const [isAtBottom, setIsAtBottom] = useState(true)
  const [logTz, setLogTz] = useState<LogTimeZone>('local')
  const [wrapLines, setWrapLines] = useState(false)

  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: filteredRecords.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 42,
    overscan: 12,
    getItemKey: (index) => filteredRecords[index]?.id ?? index,
    measureElement: (element) => element.getBoundingClientRect().height,
  })

  useEffect(() => {
    if (!scrollRef.current) return
    const element = scrollRef.current
    const onScroll = () => {
      const nearBottom = element.scrollHeight - element.scrollTop - element.clientHeight < 48
      setIsAtBottom(nearBottom)
      if (!nearBottom) setFollow(false)
      else if (!query.trim()) setFollow(true)
    }
    onScroll()
    element.addEventListener('scroll', onScroll)
    return () => element.removeEventListener('scroll', onScroll)
  }, [query])

  useEffect(() => {
    if (!follow) return
    if (filteredRecords.length === 0) return
    virtualizer.scrollToIndex(filteredRecords.length - 1, { align: 'end' })
  }, [filteredRecords.length, follow, virtualizer])

  useEffect(() => {
    if (query.trim()) setFollow(false)
    else if (isAtBottom) setFollow(true)
  }, [isAtBottom, query])

  useEffect(() => {
    virtualizer.measure()
  }, [filteredRecords, resetNonce, virtualizer, wrapLines])

  const items = virtualizer.getVirtualItems()
  const offsetTop = items[0]?.start ?? 0
  const showJump = !follow && records.length > 0
  const hasQuery = query.trim().length > 0
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
        data-service-logs-wrap={wrapLines ? 'on' : 'off'}
        data-wrap={wrapLines ? 'on' : 'off'}
      >
        <div className="serviceLogsTerminalHead" aria-hidden="true">
          <span>时间</span>
          <span>等级</span>
          <span>输出</span>
        </div>
        <div className="serviceLogsTerminalBody">
          <div className="serviceLogsViewport" ref={scrollRef}>
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
                    return (
                      <div
                        className="serviceLogRow"
                        data-following={follow ? 'true' : 'false'}
                        data-level={record.level}
                        data-wrap={wrapLines ? 'true' : 'false'}
                        key={record.id}
                        ref={virtualizer.measureElement}
                      >
                        <span className="mono serviceLogTs" title={stamp.title}>
                          <span className="serviceLogTsDate">{stamp.date}</span>
                          <span className="serviceLogTsTime">{stamp.time}</span>
                        </span>
                        <span
                          className={`mono logLvl serviceLogLevel logLvl-${record.level}`}
                          data-level={record.level}
                          title={`等级：${formatLogLevel(record.level)}（基于 ANSI 颜色与关键词推断）`}
                        >
                          {formatLogLevel(record.level)}
                        </span>
                        <span className="serviceLogMsg">
                          {record.segments.map((segment, index) => (
                            <span key={`${record.id}-${index}`} style={segment.style}>
                              {segment.text}
                            </span>
                          ))}
                        </span>
                      </div>
                    )
                  })}
                </div>
              </div>
            ) : null}
          </div>

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
