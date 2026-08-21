import { expectStory, normalizeText, waitForCondition } from "./storyAssertions";

function monitorTickCpuLabel(value: number): string {
  return value < 10 ? `${value.toFixed(1)}%` : `${value.toFixed(0)}%`;
}

function monitorPanelCpuLabel(value: number): string {
  return `${value.toFixed(1)}%`;
}

export async function assertOverviewMonitorSummary({
  canvasElement,
  monitorSummary,
}: {
  canvasElement: HTMLElement;
  monitorSummary: HTMLElement | null;
}) {
  await waitForCondition(() => Boolean(globalThis.__DOCKREV_MOCK_DEBUG__?.resourceUsageLastTick));
  const monitorDebug = globalThis.__DOCKREV_MOCK_DEBUG__;
  const historyEndCpu = monitorDebug?.resourceUsageLastSnapshot?.cpuPercent;
  const liveTickCpu = monitorDebug?.resourceUsageLastTick?.cpuPercent;
  expectStory(historyEndCpu != null && liveTickCpu != null && historyEndCpu !== liveTickCpu, "mock history end and SSE tick should differ");
  await waitForCondition(() => {
    const value = monitorSummary?.querySelector<HTMLElement>('[data-monitor-metric="CPU"]')?.getAttribute("aria-label") ?? "";
    return liveTickCpu != null && value.includes(monitorTickCpuLabel(liveTickCpu));
  });
  expectStory(Number(monitorDebug?.resourceUsageEventSourceCalls ?? 0) === 1, "overview should create one page-level resource SSE");
  await waitForCondition(() => normalizeText(canvasElement.ownerDocument.querySelector(".topbarRouteTitle")?.textContent) === "api");
}

export async function assertMonitoringResourceSync({
  canvasElement,
  monitorSummary,
  panel,
}: {
  canvasElement: HTMLElement;
  monitorSummary: HTMLElement | null;
  panel: HTMLElement;
}) {
  const monitorDebug = globalThis.__DOCKREV_MOCK_DEBUG__;
  await waitForCondition(() => Boolean(monitorDebug?.resourceUsageLastTick));
  const liveTick = monitorDebug?.resourceUsageLastTick;
  const historyEnd = monitorDebug?.resourceUsageLastSnapshot;
  expectStory(liveTick && historyEnd && liveTick.cpuPercent !== historyEnd.cpuPercent, "monitoring mock should separate history end from live tick");
  await waitForCondition(() => {
    const summaryValue = canvasElement.ownerDocument.querySelector<HTMLElement>('[data-monitor-metric="CPU"]')?.getAttribute("aria-label") ?? "";
    const panelValue = canvasElement.querySelector<HTMLElement>(".svcResourceStatValue")?.textContent ?? "";
    const chartValue = canvasElement.querySelector<HTMLElement>(".svcResourceChartCurrentValue")?.textContent ?? "";
    return liveTick != null && summaryValue.includes(monitorTickCpuLabel(liveTick.cpuPercent)) && panelValue.includes(monitorPanelCpuLabel(liveTick.cpuPercent)) && chartValue.includes(monitorPanelCpuLabel(liveTick.cpuPercent));
  });
  expectStory(Number(monitorDebug?.resourceUsageEventSourceCalls ?? 0) === 1, "monitoring page should create one resource SSE");
  const shortWindowSampleAt = panel.getAttribute("data-resource-current-sampled-at");
  expectStory(shortWindowSampleAt === liveTick?.sampledAt, "short-window panel should expose the same live sample as the topbar");
  const longWindow = canvasElement.querySelector<HTMLButtonElement>('button[value="7d"]');
  longWindow?.click();
  await waitForCondition(() => panel.getAttribute("data-resource-window") === "7d" && panel.getAttribute("data-resource-current-sampled-at") !== liveTick?.sampledAt);
  expectStory(panel.getAttribute("data-resource-current-sampled-at") !== liveTick?.sampledAt, "7d chart should keep aggregated history separate from raw SSE");
  expectStory(!normalizeText(monitorSummary?.textContent).includes("服务监控摘要"), "monitoring section should keep the compact topbar monitor summary without the subtitle");
}
