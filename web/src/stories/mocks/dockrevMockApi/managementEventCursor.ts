export type MockManagementCursor =
  | { kind: 'none' }
  | { kind: 'valid'; id: number }
  | { kind: 'resync'; reason: 'invalid_cursor' | 'generation_changed' }

export function formatMockManagementCursor(generation: string, id: number): string {
  return `${generation}:${id}`
}

export function parseMockManagementCursor(value: string, generation: string): MockManagementCursor {
  if (!value) return { kind: 'none' }
  const [cursorGeneration, sequence, ...rest] = value.split(':')
  const parsedSequence = Number(sequence)
  if (cursorGeneration !== generation) return { kind: 'resync', reason: 'generation_changed' }
  if (rest.length > 0 || !Number.isInteger(parsedSequence) || parsedSequence < 0) {
    return { kind: 'resync', reason: 'invalid_cursor' }
  }
  return { kind: 'valid', id: parsedSequence }
}
