const stackNameCollator = new Intl.Collator(undefined, {
  numeric: true,
  sensitivity: 'base',
})

export function compareStackNamesNaturally(left: string, right: string): number {
  return stackNameCollator.compare(left, right)
}
