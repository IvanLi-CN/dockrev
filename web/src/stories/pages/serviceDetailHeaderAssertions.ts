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
  const userTrigger = doc.querySelector<HTMLElement>(".topbarUserSlotTopbar .topbarUserTrigger");
  const title = doc.querySelector<HTMLElement>('[data-slot="service-title"]');
  const topbarMain = doc.querySelector<HTMLElement>(".appShellWithDetailSidebar .topbarMain");
  const actions = doc.querySelector<HTMLElement>(".topActions");
  const mobileActionTrigger = doc.querySelector<HTMLElement>(".serviceMobileActionMenuTrigger");
  const desktopActions = doc.querySelector<HTMLElement>(".serviceDesktopActions");

  const brandRect = brandLogo?.getBoundingClientRect();
  const titleRect = title?.getBoundingClientRect();
  const triggerRect = mobileActionTrigger?.getBoundingClientRect();
  const topbarRect = topbarMain?.getBoundingClientRect();
  const centerY = (rect?: DOMRect) => ((rect?.top ?? 0) + (rect?.bottom ?? 0)) / 2;

  expectStory(Boolean(menuButton && brandLogo && title && actions && mobileActionTrigger), "mobile service header should expose the service action entry");
  expectStory(!userTrigger, "mobile service header should move user identity into settings");
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
    Math.abs(centerY(triggerRect) - centerY(brandRect)) <= 2,
    "mobile service action entry and brand icon should be vertically centered",
  );
  expectStory(
    (triggerRect?.height ?? 0) >= 44 && (triggerRect?.width ?? 0) >= 44,
    "mobile service action entry should keep a 44px touch target",
  );
  expectStory(
    getComputedStyle(desktopActions ?? doc.body).display === "none",
    "mobile service header should hide the desktop split actions",
  );
  expectStory(
    (topbarRect?.height ?? Number.POSITIVE_INFINITY) <= 68,
    "mobile service header should remain a single row",
  );
  expectStory(mobileActionTrigger?.getAttribute("aria-label") === "服务操作", "mobile action entry should keep an accessible name");
}
