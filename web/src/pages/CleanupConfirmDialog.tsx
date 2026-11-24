import { useEffect, useMemo, useRef, useState } from 'react'
import { ChevronDown } from 'lucide-react'

import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Button, Mono, Pill } from '../ui'
import type { CleanupScanResponse, CleanupScanStage } from '../api'
import {
  buildCleanupConfirmGroups,
  countRenderableResources,
  formatBytes,
  formatEstimate,
  itemHasUnknownSize,
  sumKnownReclaimableBytes,
  type CleanupConfirmGroup,
} from './cleanupPageModel'

export type CleanupDialogPhase = 'loading' | 'confirm' | 'discovery' | 'applying' | 'error' | 'empty'

const STAGE_ORDER: CleanupScanStage[] = ['containers', 'images', 'volumes', 'networks', 'builder_cache']

const STAGE_LABELS: Record<CleanupScanStage, string> = {
  containers: '容器',
  images: '镜像',
  volumes: '卷',
  networks: '网络',
  builder_cache: 'Builder Cache',
}

function formatShort(ts?: string | null) {
  if (!ts) return '-'
  const date = new Date(ts)
  if (Number.isNaN(date.valueOf())) return ts
  return date.toLocaleString('zh-CN', { hour12: false })
}

function badgeFor(phase: CleanupDialogPhase, stale: boolean): { text: string; tone: 'ok' | 'warn' | 'bad' | 'muted' | 'info' } {
  if (phase === 'discovery' || phase === 'loading') return { text: '正在发现', tone: 'info' }
  if (phase === 'applying') return { text: '正在清理', tone: 'warn' }
  if (phase === 'error') return { text: '刷新失败', tone: 'bad' }
  if (phase === 'empty') return { text: '无需执行', tone: 'muted' }
  return stale ? { text: '候选已变化', tone: 'warn' } : { text: '二次确认', tone: 'bad' }
}

function CleanupConfirmResourceGroups(props: { response: CleanupScanResponse; autoExpand?: boolean }) {
  const groups = useMemo(() => buildCleanupConfirmGroups(props.response), [props.response])
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(() => new Set())
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(() => new Set())

  const isExpanded = (group: CleanupConfirmGroup) =>
    (Boolean(props.autoExpand) && group.resources.length > 0 && !collapsedGroups.has(group.key)) ||
    expandedGroups.has(group.key)

  const toggleGroup = (group: CleanupConfirmGroup) => {
    if (isExpanded(group)) {
      setCollapsedGroups((current) => new Set(current).add(group.key))
      setExpandedGroups((current) => {
        const next = new Set(current)
        next.delete(group.key)
        return next
      })
      return
    }
    setCollapsedGroups((current) => {
      const next = new Set(current)
      next.delete(group.key)
      return next
    })
    setExpandedGroups((current) => new Set(current).add(group.key))
  }

  return (
    <div className="cleanupConfirmGroups" aria-label="按资源类型查看清理候选">
      {groups.map((group: CleanupConfirmGroup) => {
        const expanded = isExpanded(group)
        const detailsId = `cleanup-confirm-resources-${group.key}`
        return (
          <section className="cleanupConfirmGroup" key={group.key}>
            <button
              aria-controls={detailsId}
              aria-expanded={expanded}
              className="cleanupConfirmGroupToggle"
              onClick={() => toggleGroup(group)}
              type="button"
            >
              <ChevronDown
                aria-hidden="true"
                className={`cleanupConfirmGroupChevron${expanded ? ' cleanupConfirmGroupChevronOpen' : ''}`}
                size={16}
              />
              <span className="cleanupConfirmGroupTitle">
                <strong>{group.label}</strong>
                <span>{group.resources.length} 项</span>
              </span>
              <span className="cleanupConfirmGroupTotals">
                <strong>已知 {formatBytes(group.bytes)}</strong>
                <span>{group.unknownCount > 0 ? `${group.unknownCount} 项大小未知` : '无未知项'}</span>
              </span>
            </button>
            <div className="cleanupConfirmGroupDetails" hidden={!expanded} id={detailsId}>
              {group.resources.length > 0 ? group.resources.map(({ resource, ownerLabel }) => (
                <div className="cleanupConfirmResource" key={`${resource.kind}:${resource.resourceId}`}>
                  <div className="cleanupConfirmResourceMain">
                    <strong className="cleanupConfirmResourceLabel">{resource.label}</strong>
                    <span className="muted cleanupConfirmResourceReason">{resource.reason}</span>
                  </div>
                  <span className="cleanupConfirmResourceOwner">{ownerLabel}</span>
                  <span className="cleanupConfirmResourceEstimate">
                    {formatEstimate(resource.estimatedReclaimableBytes, itemHasUnknownSize(resource))}
                  </span>
                </div>
              )) : (
                <div className="cleanupConfirmGroupEmpty">当前没有此类候选。</div>
              )}
            </div>
          </section>
        )
      })}
    </div>
  )
}

function DiscoveryProgress(props: { stage?: CleanupScanStage | null; response: CleanupScanResponse | null }) {
  const activeIndex = props.stage ? STAGE_ORDER.indexOf(props.stage) : -1
  const count = props.response ? countRenderableResources(props.response) : 0
  const knownBytes = props.response ? sumKnownReclaimableBytes(props.response) : 0
  return (
    <div className="cleanupDiscovery">
      <div className="cleanupDiscoveryHead">
        <div>
          <div className="cleanupDiscoveryTitle">
            <span aria-hidden="true" className="cleanupDiscoveryPulse" />
            正在发现可清理内容
          </div>
          <div className="muted">扫描完成后才允许确认；当前结果仅用于展示。</div>
        </div>
        <div aria-live="polite" className="cleanupDiscoveryCount">
          <strong>{count}</strong> 项已发现
          <span className="muted"> · 已知 {formatBytes(knownBytes)}</span>
        </div>
      </div>

      <ol className="cleanupDiscoveryStages" aria-label="扫描阶段">
        {STAGE_ORDER.map((stage, index) => {
          const state = activeIndex < 0 ? 'pending' : index < activeIndex ? 'done' : index === activeIndex ? 'active' : 'pending'
          const stateClass =
            state === 'done'
              ? 'cleanupDiscoveryStageDone'
              : state === 'active'
                ? 'cleanupDiscoveryStageActive'
                : 'cleanupDiscoveryStagePending'
          return (
            <li className={`cleanupDiscoveryStage ${stateClass}`} key={stage}>
              <span aria-hidden="true" className="cleanupDiscoveryStageDot" />
              <span>{STAGE_LABELS[stage]}</span>
            </li>
          )
        })}
      </ol>

      <div className="cleanupAlert cleanupAlertInfo">
        <div>
          {props.stage
            ? `当前阶段：${STAGE_LABELS[props.stage]}。已发现项会逐步进入下面的列表。`
            : '正在准备清理扫描，已发现项会逐步进入下面的列表。'}
        </div>
      </div>

      {props.response && countRenderableResources(props.response) > 0 ? (
        <CleanupConfirmResourceGroups autoExpand response={props.response} />
      ) : (
        <div aria-hidden="true" className="cleanupDiscoverySkeleton">
          <span />
          <span />
          <span />
        </div>
      )}
    </div>
  )
}

function ConfirmContent(props: { response: CleanupScanResponse; stale: boolean; safetyNote: string; presetLabel: string; scopeLabel: string; targetLabel: string }) {
  return (
    <div className="cleanupConfirmBody">
      <div className="cleanupConfirmSummary">
        <div className="cleanupConfirmSummaryText">
          <div className="modalLead">
            {props.targetLabel} · {props.presetLabel} · {props.scopeLabel}
          </div>
          <div className="muted">
            最新扫描：<Mono>{formatShort(props.response.scannedAt)}</Mono>
          </div>
        </div>
        <div className="cleanupConfirmEstimate">
          <div className="sectionTitle">预计释放</div>
          <div className="cleanupConfirmEstimateValue">
            {formatEstimate(props.response.estimatedReclaimableBytes, props.response.hasUnknownSize)}
          </div>
          <div className="muted cleanupConfirmEstimateHint">镜像按独占空间估算，共享层不重复计入。</div>
        </div>
      </div>

      <div className={`cleanupAlert ${props.stale ? 'cleanupAlertWarn' : 'cleanupAlertInfo'}`}>
        {props.stale ? <div>候选已变化，已替换为最新扫描结果，请再次确认。</div> : null}
        <div className={props.stale ? 'cleanupConfirmSafetyNote' : undefined}>{props.safetyNote}</div>
      </div>

      <CleanupConfirmResourceGroups response={props.response} />
    </div>
  )
}

export function CleanupConfirmDialog(props: {
  open: boolean
  title: string
  targetLabel: string
  presetLabel: string
  scopeLabel: string
  safetyNote: string
  phase: CleanupDialogPhase
  response: CleanupScanResponse | null
  stale: boolean
  stage: CleanupScanStage | null
  error: string | null
  onCancel: () => void
  onConfirm: () => void
  onRetry: () => void
}) {
  const cancelRef = useRef<HTMLButtonElement | null>(null)
  const phase = props.phase
  const canConfirm = phase === 'confirm' && props.response != null
  const badge = badgeFor(phase, props.stale)

  useEffect(() => {
    if (props.open) cancelRef.current?.focus()
  }, [props.open])

  return (
    <AlertDialog open={props.open} onOpenChange={(open) => (!open ? props.onCancel() : undefined)}>
      <AlertDialogContent className="modalCard cleanupConfirmDialogCard">
        <AlertDialogHeader className="modalHeader">
          <div className="modalTitleRow">
            <AlertDialogTitle asChild>
              <div className="modalTitle">{props.title}</div>
            </AlertDialogTitle>
            <Pill tone={badge.tone}>{badge.text}</Pill>
          </div>
          <AlertDialogDescription asChild>
            <div className="modalBody cleanupConfirmDialogBody">
              {phase === 'loading' ? (
                <div className="cleanupDiscovery">
                  <div className="cleanupDiscoveryTitle">
                    <span aria-hidden="true" className="cleanupDiscoveryPulse" />
                    正在读取最新候选
                  </div>
                  <div className="cleanupDiscoverySkeleton">
                    <span />
                    <span />
                    <span />
                  </div>
                </div>
              ) : null}

              {phase === 'discovery' ? <DiscoveryProgress stage={props.stage} response={props.response} /> : null}

              {phase === 'confirm' && props.response ? (
                <ConfirmContent
                  presetLabel={props.presetLabel}
                  response={props.response}
                  safetyNote={props.safetyNote}
                  scopeLabel={props.scopeLabel}
                  stale={props.stale}
                  targetLabel={props.targetLabel}
                />
              ) : null}

              {phase === 'applying' && props.response ? (
                <ConfirmContent
                  presetLabel={props.presetLabel}
                  response={props.response}
                  safetyNote={props.safetyNote}
                  scopeLabel={props.scopeLabel}
                  stale={props.stale}
                  targetLabel={props.targetLabel}
                />
              ) : null}

              {phase === 'applying' ? (
                <div className="cleanupAlert cleanupAlertInfo" role="status">
                  正在创建清理任务，请稍候…
                </div>
              ) : null}

              {phase === 'error' ? (
                <div className="cleanupAlert cleanupAlertError">
                  <div>{props.error ?? '清理扫描失败，请重试。'}</div>
                  <Button onClick={props.onRetry} variant="primary">重试</Button>
                </div>
              ) : null}

              {phase === 'empty' ? (
                <div className="cleanupEmptyState">
                  <div className="title">当前没有可清理项</div>
                  <div className="muted">当前规则和作用域下没有可执行候选，可以关闭窗口。</div>
                </div>
              ) : null}
            </div>
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter className="modalActions">
          <AlertDialogCancel
            className="btn btnGhost"
            disabled={phase === 'applying'}
            onClick={props.onCancel}
            ref={cancelRef}
          >
            {phase === 'discovery' ? '后台继续' : phase === 'empty' ? '关闭' : '取消'}
          </AlertDialogCancel>
          <button
            aria-busy={phase === 'applying'}
            className={`btn ${canConfirm ? 'btnDanger' : 'btnGhost'}`}
            disabled={!canConfirm}
            onClick={props.onConfirm}
            type="button"
          >
            {phase === 'discovery' || phase === 'loading'
              ? '正在发现…'
              : phase === 'applying'
                ? '清理中…'
                : '确认清理'}
          </button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
