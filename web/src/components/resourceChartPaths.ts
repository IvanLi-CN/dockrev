export type ResourceChartPoint = { x: number; y: number | null }

export type ResourceChartDomain = {
  xMin: number
  xMax: number
  yMin: number
  yMax: number
}

export type ResourceChartBox = {
  left: number
  top: number
  width: number
  height: number
}

export type ResourceChartInterpolation = 'step-after' | 'step-after-rounded'

type ScaledPoint = { x: number; y: number }

function format(value: number): string {
  return value.toFixed(2)
}

export function scaleResourceChartPoint(
  point: { x: number; y: number },
  domain: ResourceChartDomain,
  box: ResourceChartBox,
): ScaledPoint {
  const xSpan = Math.max(1, domain.xMax - domain.xMin)
  const ySpan = Math.max(1e-6, domain.yMax - domain.yMin)
  return {
    x: box.left + ((point.x - domain.xMin) / xSpan) * box.width,
    y: box.top + box.height - ((point.y - domain.yMin) / ySpan) * box.height,
  }
}

function splitSegments(points: ResourceChartPoint[], domain: ResourceChartDomain, box: ResourceChartBox): ScaledPoint[][] {
  const segments: ScaledPoint[][] = []
  let current: ScaledPoint[] = []

  for (const point of points) {
    if (point.y == null || !Number.isFinite(point.y)) {
      if (current.length) segments.push(current)
      current = []
      continue
    }
    current.push(scaleResourceChartPoint({ x: point.x, y: point.y }, domain, box))
  }

  if (current.length) segments.push(current)
  return segments
}

function traceSegment(points: ScaledPoint[], interpolation: ResourceChartInterpolation, moveToStart: boolean): string {
  const [first] = points
  if (!first) return ''

  let path = `${moveToStart ? 'M' : 'L'} ${format(first.x)} ${format(first.y)}`
  if (points.length < 2) return path

  for (let index = 1; index < points.length; index += 1) {
    const previous = points[index - 1]
    const point = points[index]
    const verticalDelta = point.y - previous.y
    const cornerRadius =
      interpolation === 'step-after-rounded'
        ? Math.min(3, (point.x - previous.x) / 4, Math.abs(verticalDelta) / 2)
        : 0

    if (cornerRadius > 0) {
      const verticalDirection = verticalDelta > 0 ? 1 : -1
      const hasFollowingSample = index < points.length - 1
      const exitRadius = hasFollowingSample ? cornerRadius : 0
      path += ` H ${format(point.x - cornerRadius)} Q ${format(point.x)} ${format(previous.y)} ${format(point.x)} ${format(previous.y + verticalDirection * cornerRadius)} V ${format(point.y - verticalDirection * exitRadius)}`
      if (exitRadius > 0) {
        path += ` Q ${format(point.x)} ${format(point.y)} ${format(point.x + exitRadius)} ${format(point.y)}`
      }
      continue
    }

    path += ` H ${format(point.x)} V ${format(point.y)}`
  }

  return path
}

export function buildResourceChartPaths(input: {
  points: ResourceChartPoint[]
  domain: ResourceChartDomain
  box: ResourceChartBox
  interpolation: ResourceChartInterpolation
  includeArea: boolean
}): { linePath: string; areaPaths: string[] } {
  const segments = splitSegments(input.points, input.domain, input.box)
  const linePath = segments.map((segment) => traceSegment(segment, input.interpolation, true)).join(' ')

  if (!input.includeArea) return { linePath, areaPaths: [] }

  const baseY = input.box.top + input.box.height
  const areaPaths = segments
    .filter((segment) => segment.length > 1)
    .map((segment) => {
      const first = segment[0]
      const last = segment[segment.length - 1]
      return `M ${format(first.x)} ${format(baseY)} ${traceSegment(segment, input.interpolation, false)} L ${format(last.x)} ${format(baseY)} Z`
    })

  return { linePath, areaPaths }
}
