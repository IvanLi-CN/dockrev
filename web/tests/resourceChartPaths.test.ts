import { describe, expect, test } from 'bun:test'
import { buildResourceChartPaths } from '../src/components/resourceChartPaths'
import { deriveResourceGapIntervals, isContinuousResourceGap } from '../src/components/resourceGapIntervals'

const domain = { xMin: 0, xMax: 30, yMin: 0, yMax: 10 }
const box = { left: 0, top: 0, width: 30, height: 10 }

function commandValues(path: string): number[] {
  return (path.match(/-?\d+(?:\.\d+)?/g) ?? []).map(Number)
}

describe('buildResourceChartPaths', () => {
  test('holds each sampled value until the next sample arrives', () => {
    const { linePath, areaPaths } = buildResourceChartPaths({
      points: [
        { x: 0, y: 2 },
        { x: 10, y: 8 },
        { x: 20, y: 4 },
        { x: 30, y: 7 },
      ],
      domain,
      box,
      interpolation: 'step-after-rounded',
      includeArea: true,
    })

    expect(linePath).not.toContain(' C ')
    expect(linePath).not.toContain(' L ')
    expect((linePath.match(/ Q /g) ?? [])).toHaveLength(5)
    expect(linePath).toContain('V 3.00')
    expect(commandValues(linePath).every((value) => value >= 0 && value <= 30)).toBe(true)
    expect(areaPaths).toHaveLength(1)
    expect(areaPaths[0]).toContain(' Q ')
  })

  test('breaks held paths at missing samples instead of connecting across the gap', () => {
    const { linePath, areaPaths } = buildResourceChartPaths({
      points: [
        { x: 0, y: 2 },
        { x: 10, y: 8 },
        { x: 20, y: null },
        { x: 30, y: 4 },
      ],
      domain,
      box,
      interpolation: 'step-after-rounded',
      includeArea: true,
    })

    expect((linePath.match(/M /g) ?? [])).toHaveLength(2)
    expect(linePath).not.toContain('20.00')
    expect(areaPaths).toHaveLength(1)
  })

  test('uses right-continuous steps for discrete process counts', () => {
    const { linePath, areaPaths } = buildResourceChartPaths({
      points: [
        { x: 0, y: 2 },
        { x: 10, y: 8 },
        { x: 20, y: 4 },
      ],
      domain,
      box,
      interpolation: 'step-after',
      includeArea: true,
    })

    expect(linePath).toBe('M 0.00 8.00 H 10.00 V 2.00 H 20.00 V 6.00')
    expect(linePath).not.toContain(' C ')
    expect(areaPaths[0]).toContain('H 10.00 V 2.00')
  })
})

describe('deriveResourceGapIntervals', () => {
  const sample = (minute: number) => ({ sampledAt: `2026-07-08T11:${String(minute).padStart(2, '0')}:00.000Z` })

  test('classifies downtime gaps and leaves one missing sample unmarked', () => {
    const lifecycle = {
      availabilityIntervals: [
        {
          operationGroupId: 'restart',
          startedAt: '2026-07-08T11:10:00.000Z',
          stoppedAt: '2026-07-08T11:05:00.000Z',
          startEventId: 2,
          stopEventId: 1,
          complete: true,
        },
      ],
      events: [],
      retentionSince: '2026-07-08T10:00:00.000Z',
    }
    const gaps = deriveResourceGapIntervals(
      [sample(0), sample(1), sample(2), sample(3), sample(4), sample(5), sample(11), sample(12), sample(13), sample(15), sample(16)],
      lifecycle,
    )

    expect(gaps).toEqual([
      { start: Date.parse('2026-07-08T11:05:00.000Z'), end: Date.parse('2026-07-08T11:11:00.000Z'), kind: 'service-stopped', missingSamples: 5 },
      { start: Date.parse('2026-07-08T11:13:00.000Z'), end: Date.parse('2026-07-08T11:15:00.000Z'), kind: 'unavailable', missingSamples: 1 },
    ])
    expect(isContinuousResourceGap(gaps[0]!)).toBe(true)
    expect(isContinuousResourceGap(gaps[1]!)).toBe(false)
  })

  test('keeps separate unavailable gaps separate', () => {
    const gaps = deriveResourceGapIntervals(
      [sample(0), sample(1), sample(2), sample(5), sample(6), sample(7), sample(10), sample(11)],
      null,
    )
    expect(gaps).toHaveLength(2)
    expect(gaps.every((gap) => gap.kind === 'unavailable')).toBe(true)
    expect(gaps.every(isContinuousResourceGap)).toBe(true)
  })
})
