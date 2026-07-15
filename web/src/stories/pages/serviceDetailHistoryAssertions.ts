const historyColumnSelectors = [
  ".serviceOperationHistoryOperation",
  ".serviceOperationHistoryStatus",
  ".serviceOperationHistoryBackup",
  ".serviceOperationHistorySource",
  ".serviceOperationHistoryTime",
  ".serviceOperationHistoryAction",
] as const;

function expectNearlyEqual(actual: number, expected: number, tolerance: number, message: string): void {
  if (Math.abs(actual - expected) > tolerance) {
    throw new globalThis.Error(`${message}: expected ${expected}, got ${actual}`);
  }
}

function historyRowCells(row: ParentNode): HTMLElement[] {
  return historyColumnSelectors.map((selector) => {
    const cell = row.querySelector<HTMLElement>(selector);
    if (!cell) throw new globalThis.Error(`missing history cell for ${selector}`);
    return cell;
  });
}

export function expectHistoryColumnsAligned(root: ParentNode): void {
  const rows = Array.from(root.querySelectorAll<HTMLElement>(".serviceOperationHistoryRow"));
  if (rows.length <= 1) throw new globalThis.Error("history alignment check needs at least two rows");

  const baselineCells = historyRowCells(rows[0]!);

  rows.slice(1).forEach((row, rowIndex) => {
    const rowCells = historyRowCells(row);
    baselineCells.forEach((baselineCell, columnIndex) => {
      const baselineRect = baselineCell.getBoundingClientRect();
      const rowRect = rowCells[columnIndex]!.getBoundingClientRect();
      expectNearlyEqual(rowRect.left, baselineRect.left, 1, `history row ${rowIndex + 2} column ${columnIndex + 1} should align with the first row`);
      expectNearlyEqual(rowRect.width, baselineRect.width, 1, `history row ${rowIndex + 2} column ${columnIndex + 1} width should match the first row`);
    });
  });
}
