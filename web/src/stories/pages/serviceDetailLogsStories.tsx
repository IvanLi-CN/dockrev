import type { ServiceLifecycleSnapshotResponse } from "../../api";
import { currentRoutePathname } from "../../routes";
import { buildDateBoundaryLogsSnapshot } from "./serviceDetailPageStoryFixtures";
import { render, type ServiceDetailStory } from "./serviceDetailStoryShared";
import { expectStory, findButton, findLink, normalizeText, waitForCondition } from "./storyAssertions";

function buildLifecycleLogSnapshot(serviceId: string): ServiceLifecycleSnapshotResponse {
  const event = (id: number, operationGroupId: string, transition: "stopped" | "started", observedAt: string) => ({
    id,
    serviceId,
    stackId: "stack-prod",
    operationGroupId,
    jobId: `job-${operationGroupId}`,
    origin: "manual_service",
    transition,
    observedAt,
    boundaryPrecision: "exact",
    evidence: { engineEvent: transition === "stopped" ? "stop" : "start" },
    details: {},
    createdAt: observedAt,
  });
  return {
    serviceId,
    since: "2026-06-30T07:00:00.000Z",
    until: "2026-06-30T09:00:00.000Z",
    retentionSince: "2026-05-31T00:00:00.000Z",
    lastEventId: 102,
    nextCursor: 102,
    events: [
      event(101, "op-logs", "stopped", "2026-06-30T08:00:01.500Z"),
      event(102, "op-logs", "started", "2026-06-30T08:00:02.500Z"),
    ],
    availabilityIntervals: [
      {
        operationGroupId: "op-logs",
        startedAt: "2026-06-30T08:00:02.500Z",
        stoppedAt: "2026-06-30T08:00:01.500Z",
        startEventId: 102,
        stopEventId: 101,
        complete: true,
      },
    ],
  };
}

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

export const LogsSectionLifecycleUnion: ServiceDetailStory = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceLogsByServiceId: {
      "svc-prod-api": {
        snapshot: buildDateBoundaryLogsSnapshot("svc-prod-api"),
        lifecycle: buildLifecycleLogSnapshot("svc-prod-api"),
      },
    },
  },
  render: render("stack-prod", "svc-prod-api", "logs", "Docker 原始日志与生命周期分隔事件合并，并可单独筛选"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes("服务已停止"));
    const lifecycleRows = Array.from(canvasElement.querySelectorAll<HTMLElement>('.serviceLifecycleSeparator[data-source="lifecycle"]'));
    expectStory(lifecycleRows.length === 2, "lifecycle union should render distinct stopped and started separators");
    expectStory(normalizeText(lifecycleRows[0]?.textContent).includes("服务已停止"), "stopped lifecycle separator should remain explicit");
    expectStory(normalizeText(lifecycleRows[1]?.textContent).includes("服务已启动"), "started lifecycle separator should remain explicit");
    expectStory(lifecycleRows.every((row) => row.getAttribute('role') === 'note'), "lifecycle separators should expose a non-log semantic role");
    expectStory(lifecycleRows.every((row) => row.querySelectorAll('.serviceLifecycleSeparatorLine').length === 2), "lifecycle separators should render horizontal rules");
    expectStory(canvasElement.querySelectorAll('.serviceLogLevelSystem').length === 0, "lifecycle events should not render a log-level badge");
    const lifecycleButton = findButton(canvasElement, "生命周期");
    expectStory(lifecycleButton, "lifecycle source filter should be available");
    const sourceButtons = Array.from(canvasElement.querySelectorAll<HTMLButtonElement>('[aria-label="日志来源"] button'));
    expectStory(sourceButtons.length === 2 && sourceButtons.every((button) => !button.disabled && button.classList.contains('btnGhost') !== button.classList.contains('btnPrimary')), "source filters should expose one clear active style without a disabled-looking lifecycle button");
    lifecycleButton.click();
    await waitForCondition(() => Boolean(canvasElement.querySelector('[data-service-logs-total-count="2"]')));
    expectStory(lifecycleButton.getAttribute('aria-pressed') === 'true' && lifecycleButton.classList.contains('btnPrimary'), "lifecycle source filter should show its active state after selection");
    expectStory(canvasElement.querySelectorAll('.serviceLogRow[data-source="docker"]').length === 0, "lifecycle filter should hide Docker rows");
  },
};

export const MobileLogsSection: ServiceDetailStory = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    viewport: { defaultViewport: "dockrevMobile" },
  },
  render: render("stack-prod", "svc-prod-api", "logs", "移动端使用底部主导航，抽屉承载服务树"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes("实时日志"));
    const doc = canvasElement.ownerDocument;
    const bottomNav = doc.querySelectorAll(".mobileBottomNavItem");
    expectStory(bottomNav.length === 5, "mobile detail page should render bottom primary navigation");

    const menuButton = doc.querySelector<HTMLButtonElement>(".mobileMenuButton");
    expectStory(menuButton, "mobile detail page should expose the service tree drawer trigger");
    menuButton?.click();
    await waitForCondition(() => normalizeText(doc.querySelector("#mobileDockrevMenu")?.textContent).includes("服务导航"));

    const drawer = doc.querySelector<HTMLElement>("#mobileDockrevMenu");
    expectStory(!drawer?.querySelector(".detailRouteTreeTitle, .detailRouteTreePath"), "mobile service drawer should remove redundant tree title and current-route copy");
    const toolbar = drawer?.querySelector<HTMLElement>(".detailRouteTreeToolbar");
    const search = toolbar?.querySelector<HTMLElement>(".detailRouteTreeSearch");
    const freshness = toolbar?.querySelector<HTMLElement>(".detailRouteTreeFreshness");
    const meta = toolbar?.querySelector<HTMLElement>(".detailRouteTreeMeta");
    expectStory(Boolean(search), "mobile service drawer should retain service tree search");
    expectStory(Boolean(freshness), "mobile service drawer should show recent scan freshness beside the counts");
    expectStory(Boolean(meta), "mobile service drawer should retain service tree counts below search");
    const toolbarRect = toolbar!.getBoundingClientRect();
    const searchRect = search!.getBoundingClientRect();
    const freshnessRect = freshness!.getBoundingClientRect();
    const metaRect = meta!.getBoundingClientRect();
    expectStory(searchRect.width >= toolbarRect.width - 1, "mobile service drawer search should use the full toolbar width");
    expectStory(freshness!.textContent?.includes("前"), "mobile service drawer recent scan should use relative time");
    expectStory(freshnessRect.top >= searchRect.bottom + 7, "mobile service drawer recent scan should sit below search");
    expectStory(Math.abs(metaRect.top - freshnessRect.top) <= 1, "mobile service drawer counts should share the recent-scan row");

    const siblingLink = findLink(doc, "web");
    expectStory(siblingLink, "mobile service drawer should include sibling services");
    siblingLink.click();
    await waitForCondition(() => currentRoutePathname() === "/services/stack-prod/svc-prod-web/logs");
  },
};
