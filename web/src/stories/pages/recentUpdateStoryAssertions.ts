import { userEvent } from "storybook/test";
import { currentRoutePathname } from "../../routes";

type ExpectStory = (condition: unknown, message: string) => void;
type WaitForCondition = (
  check: () => boolean,
  timeoutMs?: number,
) => Promise<void>;

export function navigateStoryPath(path: string): void {
  window.history.pushState({}, "", path);
  window.dispatchEvent(new PopStateEvent("popstate"));
}

export function recentUpdateLinks(root: ParentNode): HTMLButtonElement[] {
  return Array.from(root.querySelectorAll<HTMLButtonElement>(".recentUpdateLink"));
}

export async function assertRecentUpdateReasonPopoverStaysOnRoute({
  canvasElement,
  expectStory,
  routePath,
  waitForCondition,
}: {
  canvasElement: HTMLElement;
  expectStory: ExpectStory;
  routePath: string;
  waitForCondition: WaitForCondition;
}): Promise<void> {
  await waitForCondition(() => recentUpdateLinks(canvasElement).length === 3);
  const reasonRow = recentUpdateLinks(canvasElement).find((row) =>
    Boolean(row.closest(".recentUpdateRow")?.querySelector(".taskResultReasonTrigger")),
  );
  const reasonButton = reasonRow?.closest(".recentUpdateRow")?.querySelector<HTMLButtonElement>(".taskResultReasonTrigger");
  if (!reasonButton) throw new globalThis.Error("overview recent update result reason trigger missing");
  await userEvent.click(reasonButton);
  await waitForCondition(() => Boolean(canvasElement.ownerDocument.querySelector(".taskResultReasonPopover")));
  expectStory(currentRoutePathname() === routePath, "result reason trigger must not navigate away from detail route");
}

export async function assertRecentUpdateKeyboardNavigation({
  canvasElement,
  jobIndex,
  key,
  returnRoutePath,
  waitForCondition,
}: {
  canvasElement: HTMLElement;
  jobIndex: number;
  key: "{Enter}" | "[Space]";
  returnRoutePath: string;
  waitForCondition: WaitForCondition;
}): Promise<void> {
  await waitForCondition(() => recentUpdateLinks(canvasElement).length > jobIndex);
  const row = recentUpdateLinks(canvasElement)[jobIndex];
  if (!row) throw new globalThis.Error("recent update row missing");
  const jobId = row?.getAttribute("data-recent-update-job-id");
  if (!jobId) throw new globalThis.Error("recent update row should expose its target job id");
  row.focus();
  await userEvent.keyboard(key);
  await waitForCondition(() => currentRoutePathname() === `/queue/${jobId}`);
  navigateStoryPath(returnRoutePath);
  await waitForCondition(() => currentRoutePathname() === returnRoutePath);
}

export async function assertRecentUpdateClickNavigation({
  canvasElement,
  jobIndex,
  waitForCondition,
}: {
  canvasElement: HTMLElement;
  jobIndex: number;
  waitForCondition: WaitForCondition;
}): Promise<void> {
  await waitForCondition(() => recentUpdateLinks(canvasElement).length > jobIndex);
  const row = recentUpdateLinks(canvasElement)[jobIndex];
  if (!row) throw new globalThis.Error("recent update row missing");
  const jobId = row?.getAttribute("data-recent-update-job-id");
  if (!jobId) throw new globalThis.Error("recent update row should expose its target job id");
  await userEvent.click(row);
  await waitForCondition(() => currentRoutePathname() === `/queue/${jobId}`);
}
