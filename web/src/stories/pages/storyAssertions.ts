export function expectStory(condition: unknown, message: string): asserts condition {
  if (!condition) throw new globalThis.Error(message);
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export async function waitForCondition(check: () => boolean, timeoutMs = 3000): Promise<void> {
  const started = Date.now();
  while (!check()) {
    if (Date.now() - started > timeoutMs) throw new globalThis.Error("condition timeout");
    await sleep(60);
  }
}

export function normalizeText(value: string | null | undefined): string {
  return value?.replace(/\s+/g, " ").trim() ?? "";
}

export function expectNearlyEqual(actual: number, expected: number, tolerance: number, message: string): void {
  if (Math.abs(actual - expected) > tolerance) {
    throw new globalThis.Error(`${message}: expected ${expected}, got ${actual}`);
  }
}

export function findButton(root: ParentNode, text: string): HTMLButtonElement | null {
  return Array.from(root.querySelectorAll<HTMLButtonElement>("button")).find((button) => normalizeText(button.textContent) === text) ?? null;
}

export function findButtons(root: ParentNode, text: string): HTMLButtonElement[] {
  return Array.from(root.querySelectorAll<HTMLButtonElement>("button")).filter((button) => normalizeText(button.textContent) === text);
}

export function findLink(root: ParentNode, text: string): HTMLAnchorElement | null {
  return Array.from(root.querySelectorAll<HTMLAnchorElement>("a")).find((link) => normalizeText(link.textContent).includes(text)) ?? null;
}
