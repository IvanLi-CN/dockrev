import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { type ResolveGitHubPackagesTargetResponse } from '../../api'
import { Input, Mono, SelectField } from '../../ui'

import {
  type RepoListDensity,
  type RepoPickerItem,
  type RepoScopeFilter,
  type RepoSelectedFilter,
  type RepoSortKey,
  type RepoVisibilityFilter,
  formatRepoActivity,
  normalizeRepoVisibility,
  parseActivityMs,
  readRepoListDensityFromStorage,
  writeRepoListDensityToStorage,
} from './helpers'

export function GitHubPackagesRepoPicker({
  initial,
  onChange,
}: {
  initial: ResolveGitHubPackagesTargetResponse
  onChange: (repos: Array<{ fullName: string; selected: boolean }>) => void
}) {
  const [repos, setRepos] = useState<RepoPickerItem[]>(() =>
    initial.repos.map((r) => ({
      fullName: r.fullName,
      selected: r.selected,
      visibility: normalizeRepoVisibility(r.visibility),
      lastActivityAt: r.lastActivityAt ?? null,
      ghcrLinked: typeof r.ghcrLinked === 'boolean' ? r.ghcrLinked : null,
      deployed: r.deployed === true,
    })),
  )
  const [searchQuery, setSearchQuery] = useState('')
  const [scopeFilter, setScopeFilter] = useState<RepoScopeFilter>('all')
  const [selectedFilter, setSelectedFilter] = useState<RepoSelectedFilter>('all')
  const [visibilityFilter, setVisibilityFilter] = useState<RepoVisibilityFilter>('all')
  const [sortKey, setSortKey] = useState<RepoSortKey>('activity_desc')
  const [listDensity, setListDensity] = useState<RepoListDensity>(() => readRepoListDensityFromStorage())
  const dragSessionRef = useRef<{
    pointerId: number
    targetSelected: boolean
    touched: Set<string>
    captureElement: HTMLButtonElement | null
  } | null>(null)

  const setRepoSelected = useCallback((fullName: string, selected: boolean) => {
    setRepos((prev) => {
      let changed = false
      const next = prev.map((repo) => {
        if (repo.fullName !== fullName || repo.selected === selected) return repo
        changed = true
        return { ...repo, selected }
      })
      return changed ? next : prev
    })
  }, [])

  useEffect(() => {
    onChange(repos.map((r) => ({ fullName: r.fullName, selected: r.selected })))
  }, [repos, onChange])

  const filteredRepos = useMemo(() => {
    const query = searchQuery.trim().toLowerCase()

    const list = repos
      .filter((repo) => {
        if (scopeFilter === 'ghcr_linked') return repo.ghcrLinked === true
        if (scopeFilter === 'deployed') return repo.deployed
        return true
      })
      .filter((repo) => {
        if (selectedFilter === 'selected') return repo.selected
        if (selectedFilter === 'unselected') return !repo.selected
        return true
      })
      .filter((repo) => {
        if (visibilityFilter === 'public') return repo.visibility === 'public'
        if (visibilityFilter === 'private') return repo.visibility === 'private'
        return true
      })
      .filter((repo) => {
        if (!query) return true
        return repo.fullName.toLowerCase().includes(query)
      })

    list.sort((a, b) => {
      const byName = a.fullName.localeCompare(b.fullName, undefined, { sensitivity: 'base' })
      if (sortKey === 'name_asc') return byName

      const aActivity = parseActivityMs(a.lastActivityAt)
      const bActivity = parseActivityMs(b.lastActivityAt)
      if (aActivity !== null && bActivity !== null && aActivity !== bActivity) return bActivity - aActivity
      if (aActivity !== null && bActivity === null) return -1
      if (aActivity === null && bActivity !== null) return 1
      return byName
    })

    return list
  }, [repos, scopeFilter, searchQuery, selectedFilter, visibilityFilter, sortKey])

  const onWindowPointerMove = useCallback(
    (event: PointerEvent) => {
      const drag = dragSessionRef.current
      if (!drag || drag.pointerId !== event.pointerId) return
      if (event.pointerType === 'mouse' && (event.buttons & 1) === 0) {
        dragSessionRef.current = null
        if (drag.captureElement?.hasPointerCapture(drag.pointerId)) {
          drag.captureElement.releasePointerCapture(drag.pointerId)
        }
        return
      }
      if (event.pointerType === 'touch') event.preventDefault()
      const target = document.elementFromPoint(event.clientX, event.clientY)
      if (!(target instanceof HTMLElement)) return
      const hitNode = target.closest<HTMLElement>('[data-ghcr-picker-switch="true"], [data-ghcr-picker-row="true"]')
      const fullName = hitNode?.dataset.repoFullName
      if (!fullName || drag.touched.has(fullName)) return
      drag.touched.add(fullName)
      setRepoSelected(fullName, drag.targetSelected)
    },
    [setRepoSelected],
  )

  const onWindowPointerEnd = useCallback(
    function handleWindowPointerEnd(event: PointerEvent) {
      const drag = dragSessionRef.current
      if (!drag || drag.pointerId !== event.pointerId) return
      dragSessionRef.current = null
      if (drag.captureElement?.hasPointerCapture(drag.pointerId)) {
        drag.captureElement.releasePointerCapture(drag.pointerId)
      }
      window.removeEventListener('pointermove', onWindowPointerMove)
      window.removeEventListener('pointerup', handleWindowPointerEnd)
      window.removeEventListener('pointercancel', handleWindowPointerEnd)
    },
    [onWindowPointerMove],
  )

  useEffect(() => {
    return () => {
      const drag = dragSessionRef.current
      dragSessionRef.current = null
      if (drag?.captureElement?.hasPointerCapture(drag.pointerId)) {
        drag.captureElement.releasePointerCapture(drag.pointerId)
      }
      window.removeEventListener('pointermove', onWindowPointerMove)
      window.removeEventListener('pointerup', onWindowPointerEnd)
      window.removeEventListener('pointercancel', onWindowPointerEnd)
    }
  }, [onWindowPointerEnd, onWindowPointerMove])

  const onSwitchPointerDown = useCallback(
    (event: React.PointerEvent<HTMLButtonElement>, fullName: string, selected: boolean) => {
      if (event.pointerType === 'mouse' && event.button !== 0) return
      event.preventDefault()

      const targetSelected = !selected
      setRepoSelected(fullName, targetSelected)
      const captureElement = event.currentTarget
      try {
        captureElement.setPointerCapture(event.pointerId)
      } catch {
        // Some browsers/input sources may not support pointer capture for this event.
      }
      dragSessionRef.current = {
        pointerId: event.pointerId,
        targetSelected,
        touched: new Set([fullName]),
        captureElement,
      }

      window.addEventListener('pointermove', onWindowPointerMove)
      window.addEventListener('pointerup', onWindowPointerEnd)
      window.addEventListener('pointercancel', onWindowPointerEnd)
    },
    [onWindowPointerEnd, onWindowPointerMove, setRepoSelected],
  )

  const selectedCount = repos.filter((repo) => repo.selected).length
  const listClassName = listDensity === 'compact' ? 'modalList ghcrPickerList ghcrPickerListCompact' : 'modalList ghcrPickerList'

  return (
    <div className="ghcrPickerRoot">
      <div className="modalLead">
        profile <Mono>{initial.owner}</Mono> · 选择要跟踪的仓库
      </div>
      <div className="ghcrPickerLayout">
        <div className="ghcrPickerControls">
          <div className="ghcrPickerField">
            <div className="ghcrPickerFieldLabel">搜索</div>
            <Input
              className="input"
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
              placeholder="搜索 owner/repo"
            />
          </div>
          <div className="ghcrPickerField">
            <div className="ghcrPickerFieldLabel">范围筛选</div>
            <SelectField
              className="select"
              onChange={(value) => setScopeFilter(value as RepoScopeFilter)}
              options={[
                { value: 'all', label: '全部' },
                { value: 'ghcr_linked', label: '有镜像' },
                { value: 'deployed', label: '已部署' },
              ]}
              title="按 GHCR 关联或部署状态筛选"
              value={scopeFilter}
            />
          </div>
          <div className="ghcrPickerField">
            <div className="ghcrPickerFieldLabel">已添加状态</div>
            <SelectField
              className="select"
              onChange={(value) => setSelectedFilter(value as RepoSelectedFilter)}
              options={[
                { value: 'all', label: '全部' },
                { value: 'selected', label: '已添加' },
                { value: 'unselected', label: '未添加' },
              ]}
              title="按已添加状态筛选"
              value={selectedFilter}
            />
          </div>
          <div className="ghcrPickerField">
            <div className="ghcrPickerFieldLabel">可见性</div>
            <SelectField
              className="select"
              onChange={(value) => setVisibilityFilter(value as RepoVisibilityFilter)}
              options={[
                { value: 'all', label: '全部可见性' },
                { value: 'public', label: '公开' },
                { value: 'private', label: '私有' },
              ]}
              title="按可见性筛选"
              value={visibilityFilter}
            />
          </div>
          <div className="ghcrPickerField">
            <div className="ghcrPickerFieldLabel">排序方式</div>
            <SelectField
              className="select"
              onChange={(value) => setSortKey(value as RepoSortKey)}
              options={[
                { value: 'activity_desc', label: '最近活动（新→旧）' },
                { value: 'name_asc', label: '仓库名（A→Z）' },
              ]}
              title="排序方式"
              value={sortKey}
            />
          </div>
          <div className="ghcrPickerField">
            <div className="ghcrPickerFieldLabel">右侧列表布局</div>
            <button
              type="button"
              className="btn btnGhost ghcrPickerDensityButton"
              aria-pressed={listDensity === 'compact'}
              onClick={() => {
                const next = listDensity === 'compact' ? 'cozy' : 'compact'
                setListDensity(next)
                writeRepoListDensityToStorage(next)
              }}
              title="切换右侧列表布局密度"
            >
              {listDensity === 'compact' ? '紧凑（点击切回宽松）' : '宽松（点击切到紧凑）'}
            </button>
          </div>
          <div className="muted ghcrPickerSummary">
            显示 {filteredRepos.length} / {repos.length} · 已添加 {selectedCount}
          </div>
        </div>
        <div className={listClassName}>
          {filteredRepos.length === 0 ? (
            <div className="ghcrPickerEmpty">没有匹配的仓库</div>
          ) : (
            filteredRepos.map((r) => (
              <div
                key={r.fullName}
                className="modalListItem"
                data-ghcr-picker-row="true"
                data-repo-full-name={r.fullName}
              >
                <div className="modalListLeft" style={{ minWidth: 0 }}>
                  <div className="modalListTitle">
                    <span className="mono" style={{ overflowWrap: 'anywhere' }}>
                      {r.fullName}
                    </span>
                  </div>
                  <div className="ghcrPickerMeta">
                    {r.ghcrLinked ? <span>GHCR 已关联</span> : null}
                    {r.deployed ? <span>已部署</span> : null}
                    <span>{r.visibility === 'private' ? '私有' : r.visibility === 'public' ? '公开' : '可见性未知'}</span>
                    <span>{formatRepoActivity(r.lastActivityAt)}</span>
                  </div>
                </div>
                <div className="modalListRight">
                  <button
                    type="button"
                    role="switch"
                    aria-label={`切换 ${r.fullName}`}
                    aria-checked={r.selected}
                    className={r.selected ? 'switch switchButton switchButtonChecked' : 'switch switchButton'}
                    data-ghcr-picker-switch="true"
                    data-repo-full-name={r.fullName}
                    onPointerDown={(event) => onSwitchPointerDown(event, r.fullName, r.selected)}
                    onClick={(event) => {
                      if (event.detail !== 0) return
                      setRepoSelected(r.fullName, !r.selected)
                    }}
                  >
                    <span className="switchSlider" />
                  </button>
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  )
}
