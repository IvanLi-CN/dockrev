import { useCallback, useEffect, useId, useMemo, useRef, useState } from 'react'
import {
  listServiceTagSuggestions,
  putServiceComposeTag,
  type ServiceTagSuggestionItem,
} from '../api'
import { Button, Input, Mono } from '../ui'

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message
  return String(error)
}

function formatSuggestionTime(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value || '-'
  return date.toLocaleString()
}

export function ServiceComposeTagField(props: {
  busy: boolean
  currentTag: string
  serviceId: string
  onError: (message: string | null) => void
  onSaved: () => Promise<void>
}) {
  const [value, setValue] = useState(props.currentTag)
  const [open, setOpen] = useState(false)
  const [loading, setLoading] = useState(false)
  const [saving, setSaving] = useState(false)
  const [loaded, setLoaded] = useState(false)
  const [items, setItems] = useState<ServiceTagSuggestionItem[]>([])
  const [fieldError, setFieldError] = useState<string | null>(null)
  const [activeIndex, setActiveIndex] = useState(0)
  const [filterSuggestions, setFilterSuggestions] = useState(false)
  const comboId = useId()
  const closeTimerRef = useRef<number | null>(null)
  const listboxId = `${comboId}-tag-suggestions`

  const filteredItems = useMemo(() => {
    const query = filterSuggestions ? value.trim().toLowerCase() : ''
    if (!query) return items
    return items.filter((item) => item.tag.toLowerCase().includes(query))
  }, [filterSuggestions, items, value])

  useEffect(() => {
    setValue(props.currentTag)
    setOpen(false)
    setLoaded(false)
    setItems([])
    setFieldError(null)
    setActiveIndex(0)
    setFilterSuggestions(false)
  }, [props.currentTag, props.serviceId])

  useEffect(() => {
    if (!open) return
    setActiveIndex((current) =>
      Math.min(Math.max(current, 0), Math.max(filteredItems.length - 1, 0)),
    )
  }, [filteredItems.length, open])

  const loadSuggestions = useCallback(async () => {
    if (loaded || loading) return
    setLoading(true)
    setFieldError(null)
    try {
      const resp = await listServiceTagSuggestions(props.serviceId)
      setItems(resp.items)
      setLoaded(true)
    } catch (error: unknown) {
      setFieldError(errorMessage(error))
    } finally {
      setLoading(false)
    }
  }, [loaded, loading, props.serviceId])

  const openSuggestions = useCallback(() => {
    if (closeTimerRef.current != null) {
      window.clearTimeout(closeTimerRef.current)
      closeTimerRef.current = null
    }
    setOpen(true)
    void loadSuggestions()
  }, [loadSuggestions])

  const scheduleClose = useCallback(() => {
    closeTimerRef.current = window.setTimeout(() => {
      setOpen(false)
    }, 120)
  }, [])

  const selectSuggestion = useCallback((item: ServiceTagSuggestionItem) => {
    setValue(item.tag)
    setOpen(false)
    setActiveIndex(0)
    setFilterSuggestions(false)
  }, [])

  const save = useCallback(async () => {
    const next = value.trim()
    setFieldError(null)
    props.onError(null)
    if (!next) {
      setFieldError('tag 不能为空')
      return
    }
    setSaving(true)
    try {
      await putServiceComposeTag(props.serviceId, next)
      setLoaded(false)
      setItems([])
      setOpen(false)
      await props.onSaved()
    } catch (error: unknown) {
      setFieldError(errorMessage(error))
    } finally {
      setSaving(false)
    }
  }, [props, value])

  const disabled = props.busy || saving

  return (
    <div className="serviceTagEditor">
      <div className="serviceTagEditorHeader">
        <div>
          <div className="label">部署 tag</div>
          <div className="muted">写回原始 Compose 文件；保存后不会自动执行 compose up。</div>
        </div>
        <div className="chipStatic">
          当前 <Mono>{props.currentTag || '-'}</Mono>
        </div>
      </div>
      <div className="serviceTagEditorControls">
        <div className="serviceTagInputWrap">
          <Input
            aria-activedescendant={
              open && filteredItems[activeIndex] ? `${listboxId}-option-${activeIndex}` : undefined
            }
            aria-autocomplete="list"
            aria-controls={listboxId}
            aria-expanded={open}
            autoComplete="off"
            className="input"
            disabled={disabled}
            onBlur={scheduleClose}
            onChange={(event) => {
              setValue(event.target.value)
              setOpen(true)
              setActiveIndex(0)
              setFilterSuggestions(true)
              void loadSuggestions()
            }}
            onFocus={openSuggestions}
            onKeyDown={(event) => {
              if (event.key === 'ArrowDown') {
                event.preventDefault()
                setOpen(true)
                void loadSuggestions()
                setActiveIndex((current) =>
                  Math.min(current + 1, Math.max(filteredItems.length - 1, 0)),
                )
                return
              }
              if (event.key === 'ArrowUp') {
                event.preventDefault()
                setOpen(true)
                setActiveIndex((current) => Math.max(current - 1, 0))
                return
              }
              if (event.key === 'Enter') {
                if (open && filteredItems[activeIndex]) {
                  event.preventDefault()
                  selectSuggestion(filteredItems[activeIndex])
                  return
                }
                event.preventDefault()
                void save()
                return
              }
              if (event.key === 'Escape') setOpen(false)
            }}
            placeholder="例如 5.2.3 或 stable"
            role="combobox"
            value={value}
          />
          {open ? (
            <div className="serviceTagSuggestionMenu" id={listboxId} role="listbox">
              {loading ? <div className="serviceTagSuggestionEmpty">加载历史 tag…</div> : null}
              {!loading && fieldError ? <div className="serviceTagSuggestionEmpty">{fieldError}</div> : null}
              {!loading && !fieldError && filteredItems.length === 0 ? (
                <div className="serviceTagSuggestionEmpty">
                  {items.length === 0 ? '暂无历史 tag' : '没有匹配的历史 tag'}
                </div>
              ) : null}
              {!loading && !fieldError
                ? filteredItems.map((item, index) => (
                    <button
                      aria-selected={index === activeIndex}
                      className={`serviceTagSuggestionItem${index === activeIndex ? ' active' : ''}`}
                      id={`${listboxId}-option-${index}`}
                      key={`${item.tag}-${item.lastUsedAt}`}
                      onMouseDown={(event) => event.preventDefault()}
                      onClick={() => selectSuggestion(item)}
                      onMouseEnter={() => setActiveIndex(index)}
                      role="option"
                      type="button"
                    >
                      <span className="mono monoPrimary">{item.tag}</span>
                      <span className="muted">{formatSuggestionTime(item.lastUsedAt)}</span>
                    </button>
                  ))
                : null}
            </div>
          ) : null}
        </div>
        <Button
          variant="primary"
          disabled={disabled || value.trim() === props.currentTag.trim()}
          onClick={() => void save()}
        >
          {saving ? '保存中…' : '保存 tag'}
        </Button>
      </div>
      {fieldError ? <div className="serviceTagFieldError">{fieldError}</div> : null}
    </div>
  )
}
