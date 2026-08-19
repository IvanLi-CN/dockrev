import { buildDateBoundaryLogsSnapshot } from "./serviceDetailPageStoryFixtures";
import { render, type ServiceDetailStory } from "./serviceDetailStoryShared";
import { expectStory, findButton, normalizeText, waitForCondition } from "./storyAssertions";

export function expectDesktopLogTimestampLayout(canvasElement: HTMLElement, row: HTMLElement | null): void {
  const timestamp = row?.querySelector<HTMLElement>(".serviceLogTs");
  const time = timestamp?.querySelector<HTMLElement>(".serviceLogTsTime");
  const date = timestamp?.querySelector<HTMLElement>(".serviceLogTsDate");
  const headerTime = canvasElement.querySelector<HTMLElement>(".serviceLogsTerminalHead > span");
  expectStory(timestamp && time && date && headerTime, "logs timestamp cells should render both time and date");
  expectStory(Boolean(time.compareDocumentPosition(date) & Node.DOCUMENT_POSITION_FOLLOWING), "logs should render time before date");
  expectStory(time.getBoundingClientRect().top < date.getBoundingClientRect().top, "logs should place time above date");
  expectStory(Math.abs(timestamp.getBoundingClientRect().left - headerTime.getBoundingClientRect().left) <= 1, "logs timestamp body should align with the time header");
}

export const MobileLogsTimestampLayout: ServiceDetailStory = {
  parameters: { dockrevApiScenario: "dashboard-demo", viewport: { defaultViewport: "dockrevMobile" } },
  render: render("stack-prod", "svc-prod-api", "logs", "移动端日志时间列布局"),
};

export const DesktopLogsTimestampLayout: ServiceDetailStory = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod", "svc-prod-api", "logs", "桌面端日志时间列布局"),
};

export const LogsSectionDateBoundaries: ServiceDetailStory = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceLogsByServiceId: {
      "svc-prod-api": { snapshot: buildDateBoundaryLogsSnapshot("svc-prod-api"), eventsPayload: ": keep-alive\n\n" },
    },
    viewport: { defaultViewport: "dockrevMobile" },
  },
  render: render("stack-prod", "svc-prod-api", "logs", "移动端日期分隔线跳过无效时间戳并响应 UTC 换日"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes("same day after invalid timestamp"));
    const rows = Array.from(canvasElement.querySelectorAll<HTMLElement>(".serviceLogRow"));
    expectStory(rows.length === 4, "date-boundary fixture should render four logical rows");
    expectStory(rows[0]?.getAttribute("data-date-divider") === "true", "first valid row should show a date divider");
    expectStory(rows[1]?.getAttribute("data-date-divider") === "false", "invalid timestamp should not show a date divider");
    expectStory(rows[2]?.getAttribute("data-date-divider") === "false", "same date after an invalid timestamp should not repeat the divider");
    expectStory(rows[0]?.getAttribute("data-log-date") === rows[2]?.getAttribute("data-log-date"), "same-day valid rows should keep the same local date");
    const invalidStamp = rows[1]?.querySelector<HTMLElement>('.serviceLogTs[data-valid="false"]');
    expectStory(invalidStamp?.querySelector(".serviceLogTsTime") == null, "invalid timestamp fallback should not create an empty time line");
    expectStory(normalizeText(invalidStamp?.querySelector(".serviceLogTsDate")?.textContent) === "not-a-timestamp", "invalid timestamp should stay readable");
    findButton(canvasElement, "UTC")?.click();
    await waitForCondition(() => rows[3]?.getAttribute("data-log-date") === "2026-06-30");
    const dividerTexts = rows.filter((row) => row.getAttribute("data-date-divider") === "true").map((row) => normalizeText(row.querySelector(".serviceLogDateDivider")?.textContent));
    expectStory(JSON.stringify(dividerTexts) === JSON.stringify(["2026-06-29", "2026-06-30"]), "UTC date dividers should mark the UTC day boundary once");
  },
};
