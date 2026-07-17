import { normalizeText } from "./storyAssertions";

type ExpectStory = (condition: unknown, message: string) => void;

const MONITOR_METRIC_LABELS = ["CPU", "内存", "磁盘读", "磁盘写", "下载", "上传"];

export function expectCompactMonitorSummaryRow({
  monitorRow,
  expectedServiceName,
  expectStory,
}: {
  monitorRow: HTMLElement | null | undefined;
  expectedServiceName: string;
  expectStory: ExpectStory;
}) {
  const monitorRowText = normalizeText(monitorRow?.textContent);
  const monitorMetricChips = Array.from(monitorRow?.querySelectorAll<HTMLElement>("[data-monitor-metric]") ?? []);
  const monitorMetricLabels = monitorMetricChips.map((chip) => chip.getAttribute("data-monitor-metric") ?? "");

  expectStory(
    monitorRowText.includes(expectedServiceName) && JSON.stringify(monitorMetricLabels) === JSON.stringify(MONITOR_METRIC_LABELS),
    "monitor summary row should expose six icon-backed metric chips",
  );
  expectStory(
    monitorMetricChips.every((chip) => chip.querySelector(".svcDetailMonitorMetricIcon svg")),
    "monitor summary row should render icons instead of visible text labels",
  );
  expectStory(
    !MONITOR_METRIC_LABELS.some((label) => monitorRowText.includes(label)),
    "monitor summary row should hide the metric labels from visible text",
  );
  expectStory(!monitorRow?.querySelector('[data-monitor-state="sample-time"]'), "monitor summary row should remove the sample-time chip");
  expectStory(!monitorRowText.includes("服务监控摘要"), "monitor summary row should remove the redundant subtitle");
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
  expectStory(!canvasElement.ownerDocument.querySelector(".pageHead"), `${context} should not render a redundant page title block`);
  expectStory(!canvasElement.querySelector(".detailHeroCardService"), `${context} should not render the separate service hero card`);
  expectStory(!canvasElement.querySelector(".detailHeroStatusCard"), `${context} should not render the old nested hero status card`);
  expectStory(!canvasElement.querySelector(".detailHeroMetaGrid"), `${context} should not render the old header meta grid`);
}

export function expectMobileMonitorMetricsGrid({
  monitorRow,
  monitorMetrics,
  monitorMetricChips,
  expectStory,
}: {
  monitorRow: HTMLElement | null | undefined;
  monitorMetrics: HTMLElement | null | undefined;
  monitorMetricChips: HTMLElement[];
  expectStory: ExpectStory;
}) {
  expectStory((monitorRow?.scrollWidth ?? 0) <= (monitorRow?.clientWidth ?? 0) + 1, "mobile monitor summary row should wrap instead of overflowing horizontally");
  expectStory(Boolean(monitorMetrics), "mobile history should render the monitor metrics grid");
  expectStory(getComputedStyle(monitorMetrics ?? document.body).display === "grid", "mobile monitor metrics should switch to a grid layout");
  expectStory(monitorMetricChips.length === 6, "mobile history should keep six monitor metric chips");

  const metricRects = monitorMetricChips.map((chip) => chip.getBoundingClientRect());
  const distinctLefts = Array.from(new Set(metricRects.map((rect) => Math.round(rect.left))));
  const distinctTops = Array.from(new Set(metricRects.map((rect) => Math.round(rect.top))));
  const sameColumn = (a: number, b: number) => Math.abs(metricRects[a].left - metricRects[b].left) <= 2;
  const stacked = (a: number, b: number) => metricRects[a].top < metricRects[b].top - 2;

  expectStory(distinctLefts.length === 3 && distinctTops.length === 2, "mobile monitor metrics should form a two-row, three-column grid");
  expectStory(
    metricRects.length === 6 &&
      sameColumn(0, 1) &&
      sameColumn(2, 3) &&
      sameColumn(4, 5) &&
      stacked(0, 1) &&
      stacked(2, 3) &&
      stacked(4, 5),
    "mobile monitor metrics should pair CPU/内存, 磁盘读/写, 下载/上传 by column",
  );
}
