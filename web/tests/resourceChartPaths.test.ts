import { describe, expect, test } from 'bun:test'
import { buildResourceChartPaths } from '../src/components/resourceChartPaths'

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
