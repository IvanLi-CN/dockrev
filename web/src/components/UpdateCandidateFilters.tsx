import type { RowStatus } from '../updateStatus'
import { FilterChips } from '../Shell'

export type UpdateCandidateFilter = 'all' | Exclude<RowStatus, 'ok'>
export type UpdateCandidateCounts = Record<Exclude<RowStatus, 'ok'>, number>

export function UpdateCandidateFilters(props: {
  value: UpdateCandidateFilter
  onChange: (v: UpdateCandidateFilter) => void
  total: number
  counts: UpdateCandidateCounts
}) {
  return (
    <FilterChips
      value={props.value}
      onChange={props.onChange}
      items={[
        { key: 'all', label: '全部', count: props.total },
        { key: 'updatable', label: '可更新', count: props.counts.updatable },
        { key: 'hint', label: '需确认', count: props.counts.hint },
        { key: 'crossTag', label: '跨标签版本', count: props.counts.crossTag },
        { key: 'archMismatch', label: '架构不匹配', count: props.counts.archMismatch },
        { key: 'blocked', label: '被阻止', count: props.counts.blocked },
      ]}
    />
  )
}
