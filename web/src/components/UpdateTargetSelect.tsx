import { useEffect, useMemo, useRef, useState } from 'react'
import { listServiceCandidates, type ServiceCandidateOption } from '../api'
import { Mono } from '../ui'
import { tagSeriesMatches } from '../updateStatus'

export type SelectedTarget = { tag: string; digest?: string | null }

function isSelectable(opt: ServiceCandidateOption): boolean {
  if (!opt.digest) return false
  if (opt.ignored) return false
  if (opt.archMatch === 'mismatch') return false
  return true
}

export function UpdateTargetSelect(props: {
  serviceId: string
  currentTag: string
  initialTag?: string | null
  initialDigest?: string | null
  variant?: 'block' | 'inline'
  showLabel?: boolean
  showComparison?: boolean
  onChange: (next: SelectedTarget) => void
}) {
  const { serviceId, currentTag, initialTag, initialDigest, onChange } = props
  const onChangeRef = useRef(onChange)
  useEffect(() => {
    onChangeRef.current = onChange
  }, [onChange])
  const [opts, setOpts] = useState<ServiceCandidateOption[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [selectedTag, setSelectedTag] = useState<string | null>(null)

  useEffect(() => {
    let alive = true
    void (async () => {
      setError(null)
      try {
        const data = await listServiceCandidates(serviceId)
        if (!alive) return
        setOpts(data)
      } catch (e: unknown) {
        if (!alive) return
        setError(e instanceof Error ? e.message : String(e))
        setOpts([])
      }
    })()
    return () => {
      alive = false
    }
  }, [serviceId])

  const selectable = useMemo(() => (opts ?? []).filter((o) => isSelectable(o)), [opts])
  const selectDisabled = selectable.length <= 1
  const variant = props.variant ?? 'block'
  const showLabel = props.showLabel !== false
  const showComparison = props.showComparison !== false

  const defaultTag = useMemo(() => {
    if (!opts) return initialTag ?? '-'
    const preferred = opts.find((o) => isSelectable(o) && tagSeriesMatches(currentTag, o.tag) === true) ?? null
    const fallback = opts.find((o) => isSelectable(o)) ?? null
    const base = preferred ?? fallback
    return base?.tag ?? initialTag ?? '-'
  }, [currentTag, initialTag, opts])

  const effectiveTag = useMemo(() => {
    if (!opts) return defaultTag
    if (selectedTag && opts.some((o) => o.tag === selectedTag)) return selectedTag
    return defaultTag
  }, [defaultTag, opts, selectedTag])

  const effectiveDigest = useMemo(() => {
    if (!opts) return initialDigest ?? null
    const hit = opts.find((o) => o.tag === effectiveTag) ?? null
    return hit?.digest ?? initialDigest ?? null
  }, [effectiveTag, initialDigest, opts])

  useEffect(() => {
    if (!opts) return
    if (selectable.length === 0) return
    onChangeRef.current({ tag: effectiveTag, digest: effectiveDigest })
  }, [effectiveDigest, effectiveTag, opts, selectable.length])

  const selectNode =
    opts == null ? (
      <span className="muted">加载中…</span>
    ) : error ? (
      <span className="muted">候选列表不可用</span>
    ) : selectable.length === 0 ? (
      <span className="muted">无可选候选</span>
    ) : selectDisabled ? (
      <Mono>{`${effectiveTag}`}</Mono>
    ) : (
      <select
        className="select"
        value={effectiveTag}
        onChange={(e) => {
          const nextTag = e.target.value
          setSelectedTag(nextTag)
          const hit = (opts ?? []).find((o) => o.tag === nextTag) ?? null
          onChangeRef.current({ tag: nextTag, digest: hit?.digest ?? null })
        }}
      >
        {opts?.map((o) => {
          const disabled = !isSelectable(o)
          const series = tagSeriesMatches(currentTag, o.tag)
          const prefix = series === true ? '✓ ' : series === false ? '· ' : '? '
          const suffix = o.ignored ? ' (ignored)' : o.archMatch === 'mismatch' ? ' (arch mismatch)' : o.digest ? '' : ' (no digest)'
          return (
            <option key={o.tag} value={o.tag} disabled={disabled}>
              {`${prefix}${o.tag}${suffix}`}
            </option>
          )
        })}
      </select>
    )

  if (variant === 'inline') {
    return selectNode
  }

  return (
    <div className="targetSelect">
      <div className="targetSelectTop">
        {showLabel ? <div className="targetSelectLabel">目标版本</div> : <div />}
        <div className="targetSelectValue">
          {selectNode}
        </div>
      </div>
      {showComparison ? (
        <div className="muted">
          <span className="mono">{currentTag}</span> →{' '}
          <span className="mono" title={effectiveDigest ? `${effectiveTag}@${effectiveDigest}` : effectiveTag}>
            {effectiveTag}
          </span>
        </div>
      ) : null}
    </div>
  )
}
