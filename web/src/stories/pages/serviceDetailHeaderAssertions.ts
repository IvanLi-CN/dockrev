import { normalizeText } from "./storyAssertions";

type ExpectStory = (condition: unknown, message: string) => void;

const MONITOR_METRIC_LABELS = ["CPU", "内存", "磁盘读", "磁盘写", "下载", "上传"];
const MONITOR_GROUPS = ["compute", "disk", "network"];

export function expectTopbarMonitorSummary({
  monitorSummary,
  expectStory,
}: {
  monitorSummary: HTMLElement | null | undefined;
  expectStory: ExpectStory;
}) {
  const monitorSummaryText = normalizeText(monitorSummary?.textContent);
  const monitorMetricChips = Array.from(monitorSummary?.querySelectorAll<HTMLElement>("[data-monitor-metric]") ?? []);
  const monitorMetricLabels = monitorMetricChips.map((chip) => chip.getAttribute("data-monitor-metric") ?? "");
  const monitorGroups = Array.from(monitorSummary?.querySelectorAll<HTMLElement>("[data-monitor-group]") ?? []);
  const monitorGroupKeys = monitorGroups.map((group) => group.getAttribute("data-monitor-group") ?? "");

  expectStory(
    JSON.stringify(monitorMetricLabels) === JSON.stringify(MONITOR_METRIC_LABELS),
    "topbar monitor summary should expose six icon-backed metric chips",
  );
  expectStory(
    JSON.stringify(monitorGroupKeys) === JSON.stringify(MONITOR_GROUPS),
    "topbar monitor summary should keep compute, disk, and network as indivisible groups",
  );
  expectStory(
    monitorMetricChips.every((chip) => chip.querySelector(".topbarServiceMonitorMetricIcon svg")),
    "topbar monitor summary should render icons instead of visible text labels",
  );
  expectStory(
    !MONITOR_METRIC_LABELS.some((label) => monitorSummaryText.includes(label)),
    "topbar monitor summary should hide the metric labels from visible text",
  );
  expectStory(!monitorSummary?.querySelector('[data-monitor-state="sample-time"]'), "topbar monitor summary should remove the sample-time chip");
  expectStory(!monitorSummaryText.includes("服务监控摘要"), "topbar monitor summary should remove the redundant subtitle");
}

export function expectNoLegacyServiceDetailHero({
  canvasElement,
  expectStory,
  context,
}: {
  canvasElement: HTMLElement;
  expectStory: ExpectStory;
  context: string;
}) {
  expectStory(!canvasElement.querySelector(".detailHeroCardService"), `${context} should not render the separate service hero card`);
  expectStory(!canvasElement.querySelector(".detailHeroStatusCard"), `${context} should not render the old nested hero status card`);
  expectStory(!canvasElement.querySelector(".detailHeroMetaGrid"), `${context} should not render the old header meta grid`);
}

export function expectMobileTopbarMonitorHidden({
  monitorSummary,
  expectStory,
}: {
  monitorSummary: HTMLElement | null | undefined;
  expectStory: ExpectStory;
}) {
  expectStory(
    monitorSummary?.closest<HTMLElement>('[data-slot="service-metrics"]') != null,
    "mobile detail should keep the service monitor summary in the topbar context",
  );
  expectStory(
    getComputedStyle(monitorSummary?.closest<HTMLElement>('[data-slot="service-metrics"]') ?? document.body).display === "none",
    "mobile detail should hide monitor groups before they crowd the topbar",
  );
}

export function expectMobileServiceHeaderLayers({
  doc,
  expectStory,
}: {
  doc: Document;
  expectStory: ExpectStory;
}) {
  const menuButton = doc.querySelector<HTMLElement>(".mobileMenuButton");
  const brandLogo = doc.querySelector<HTMLElement>(".topbarLeft .brandLogoThemeSwitch");
  const userTrigger = doc.querySelector<HTMLElement>(".topbarUserSlot .topbarUserTrigger");
  const title = doc.querySelector<HTMLElement>('[data-slot="service-title"]');
  const actions = doc.querySelector<HTMLElement>(".topActions");
  const stackAction = doc.querySelector<HTMLElement>(".serviceStackDetailAction");
  const stackActionLabel = stackAction?.querySelector<HTMLElement>(".serviceStackDetailActionLabel");
  const actionButtons = Array.from(actions?.querySelectorAll<HTMLElement>("button") ?? []);

  const brandRect = brandLogo?.getBoundingClientRect();
  const titleRect = title?.getBoundingClientRect();
  const actionsRect = actions?.getBoundingClientRect();
  const centerY = (rect?: DOMRect) => ((rect?.top ?? 0) + (rect?.bottom ?? 0)) / 2;

  expectStory(Boolean(menuButton && brandLogo && userTrigger && title && actions), "mobile service header should expose both global and service layers");
  expectStory(
    (brandRect?.width ?? Number.POSITIVE_INFINITY) <= 36,
    "mobile service header should use the icon-only brand mark",
  );
  expectStory(
    (titleRect?.left ?? 0) >= (brandRect?.right ?? Number.MAX_SAFE_INTEGER) &&
      (titleRect?.left ?? Number.POSITIVE_INFINITY) - (brandRect?.right ?? 0) <= 12,
    "mobile service name should sit immediately to the right of the brand icon",
  );
  expectStory(
    Math.abs(centerY(titleRect) - centerY(brandRect)) <= 2,
    "mobile service name and brand icon should be vertically centered",
  );
  expectStory(
    (actionsRect?.top ?? 0) >= (menuButton?.getBoundingClientRect().bottom ?? Number.MAX_SAFE_INTEGER) - 1,
    "mobile service actions should occupy the second header row",
  );
  expectStory(
    (actions?.scrollWidth ?? 0) <= (actions?.clientWidth ?? 0) + 1,
    "mobile service actions should fit without a horizontal scroller",
  );
  expectStory(
    actionButtons.every((button) => button.getBoundingClientRect().height >= 44),
    "mobile service actions should keep 44px touch targets",
  );
  expectStory(stackAction?.getAttribute("aria-label") === "Stack 详情", "mobile stack action should keep an accessible name");
  expectStory(
    getComputedStyle(stackActionLabel ?? doc.body).display === "none",
    "mobile stack action should use its icon instead of consuming a text column",
  );
}
