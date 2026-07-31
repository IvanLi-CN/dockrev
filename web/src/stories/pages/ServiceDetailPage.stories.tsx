import type { Meta } from "@storybook/react";
import { ServiceDetailPage } from "../../pages/ServiceDetailPage";
import { currentRoutePathname } from "../../routes";
import { withDockrevMockApi } from "../mocks/withDockrevMockApi";
import { expectMobileTopbarMonitorHidden, expectNoLegacyServiceDetailHero, expectTopbarMonitorSummary } from "./serviceDetailHeaderAssertions";
import { expectHistoryColumnsAligned } from "./serviceDetailHistoryAssertions";
import { buildLongLogsSnapshot, buildMultilineLogsSnapshot, historyReleaseNotes, paginatedHistoryJobs, partialHistoryBackupRecords } from "./serviceDetailPageStoryFixtures";
import { assertRecentUpdateKeyboardNavigation, assertRecentUpdateReasonPopoverStaysOnRoute } from "./recentUpdateStoryAssertions";
import { drawerText, findActionButton, findHistoryRowByJobId, findLogRowContaining, findSectionCard, findTab, render, tabLabels, type ServiceDetailStory } from "./serviceDetailStoryShared";
export { DockrevVersionsSelfUpgrade, DockrevVersionsSelfUpgradeVisual, DockrevVersionsSelfUpgradeOffline, MobileVersionsSection, VersionsSection, VersionsSectionActionGuard } from "./serviceDetailVersionsStories";
import { expectNearlyEqual, expectStory, findButton, findButtons, findLink, normalizeText, waitForCondition } from "./storyAssertions";

const meta: Meta<typeof ServiceDetailPage> = {
  title: "Pages/ServiceDetailPage",
  component: ServiceDetailPage,
  decorators: [withDockrevMockApi],
  tags: ["autodocs"],
  parameters: { layout: "fullscreen" },
};

export default meta;
type Story = ServiceDetailStory;

export const OverviewDefault: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod", "svc-prod-api", "overview", "旧链接默认落到概览；保留共享顶部动作与最近更新记录"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes("最近更新记录"));
    await waitForCondition(() => normalizeText(canvasElement.ownerDocument.body.textContent).includes("服务列表"));
    const monitorSummary = canvasElement.ownerDocument.querySelector<HTMLElement>('[data-service-detail-context="monitor-summary"]');
    const statusRail = canvasElement.querySelector<HTMLElement>('[data-service-detail-context="status-summary"]');
    const statusSummary = statusRail?.querySelector<HTMLElement>(".svcBannerDetail");
    const statusSummaryText = normalizeText(statusSummary?.textContent);
    expectStory(currentRoutePathname() === "/services/stack-prod/svc-prod-api", "legacy overview route should stay canonical");
    await waitForCondition(() => findTab(canvasElement, "overview")?.getAttribute("data-state") === "active");
    expectStory(findTab(canvasElement, "overview")?.getAttribute("data-state") === "active", "overview tab should be active");
    expectStory(JSON.stringify(tabLabels(canvasElement)) === JSON.stringify(["概览", "版本", "更新记录", "监控", "日志", "备份", "设置"]), "service detail tabs should follow the reordered sequence");
    expectStory(!normalizeText(canvasElement.textContent).includes("资源监控"), "overview should not render monitoring panel");
    expectStory(!findSectionCard(canvasElement, "auto-policy"), "overview should not render settings cards");
    expectStory(findButton(canvasElement, "Stack 详情"), "stack detail top action missing");
    expectStory(Boolean(canvasElement.ownerDocument.querySelector(".detailRouteServiceLinkActive")), "detail service tree should highlight the current service");
    expectStory(Boolean(monitorSummary), "topbar monitor summary missing");
    expectStory(!canvasElement.querySelector('[data-service-detail-context="monitor-summary"]'), "service detail body should not retain the monitor summary row");
    expectStory(Boolean(statusRail), "shared status rail missing");
    expectStory(Boolean(statusSummary), "shared status summary card detail missing");
    expectTopbarMonitorSummary({ monitorSummary, expectStory });
    await waitForCondition(() => normalizeText(canvasElement.ownerDocument.querySelector(".topbarRouteTitle")?.textContent) === "api");
    expectStory(!canvasElement.ownerDocument.querySelector(".pageHead .h1"), "service detail must not repeat the service name in the body");
    expectStory(!normalizeText(monitorSummary?.textContent).includes("api"), "topbar monitor summary should not repeat the service name");
    expectStory(
      statusSummaryText.includes("当前 5.2.1") &&
        statusSummaryText.includes("目标 5.2.3") &&
        statusSummaryText.includes("跨"),
      "shared status summary should only keep current version, target version, and version span",
    );
    expectStory(
      !statusSummaryText.includes("sha256") &&
        !statusSummaryText.includes("linux/amd64") &&
        !statusSummaryText.includes("规则") &&
        !statusSummaryText.includes("原因"),
      "shared status summary should remove digest, arch, and rule-detail text",
    );
    expectStory(!statusRail?.querySelector(".svcDetailSummaryName"), "shared status rail should not repeat the service name");
    expectStory(!normalizeText(statusRail?.textContent).includes("prod"), "shared status rail should not repeat the stack pill");
    expectNoLegacyServiceDetailHero({ canvasElement, expectStory, context: "service detail" });
    expectStory(Boolean(findSectionCard(canvasElement, "service-identifiers")), "overview should carry the service identifiers card");
    expectStory(normalizeText(findSectionCard(canvasElement, "service-identifiers")?.textContent).includes("Image Ref"), "service identifiers card should include image ref");
    await assertRecentUpdateReasonPopoverStaysOnRoute({
      canvasElement,
      expectStory,
      routePath: "/services/stack-prod/svc-prod-api",
      waitForCondition,
    });
    await assertRecentUpdateKeyboardNavigation({
      canvasElement,
      jobIndex: 1,
      key: "{Enter}",
      returnRoutePath: "/services/stack-prod/svc-prod-api",
      waitForCondition,
    });
    await assertRecentUpdateKeyboardNavigation({
      canvasElement,
      jobIndex: 2,
      key: "[Space]",
      returnRoutePath: "/services/stack-prod/svc-prod-api",
      waitForCondition,
    });
  },
};

export const OverviewRecentUpdateEvidence: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod", "svc-prod-api", "overview", "最近更新记录摘要卡保持概览布局，并支持任务详情直达。"),
};

export const ArchivedServiceNavigation: Story = {
  parameters: { dockrevApiScenario: "archived-stack-detail-navigation" },
  render: render("stack-lab", "svc-lab-arch", "overview", "归档服务详情也必须保留同一份 Stack → Service 树与当前节点高亮。"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => normalizeText(doc.body.textContent).includes("vaultwarden"));
    expectStory(currentRoutePathname() === "/services/stack-lab/svc-lab-arch", "archived service detail route should stay canonical");
    expectStory(doc.querySelector(".detailRouteStackLinkCurrent")?.textContent?.includes("home-lab"), "archived stack should stay visible in the detail tree");
    expectStory(doc.querySelector(".detailRouteServiceLinkActive")?.textContent?.includes("vaultwarden"), "archived service should stay highlighted in the detail tree");
    expectStory(
      Object.keys(globalThis.__DOCKREV_MOCK_DEBUG__?.stackDetailCallsById ?? {}).every((id) => id === "stack-lab"),
      "detail tree should not prefetch unrelated stack details",
    );
  },
};

export const MonitoringSection: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod", "svc-prod-api", "monitoring", "监控子页只承载资源监控面板"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes("资源监控"));
    const monitorSummary = canvasElement.ownerDocument.querySelector<HTMLElement>('[data-service-detail-context="monitor-summary"]');
    expectStory(currentRoutePathname() === "/services/stack-prod/svc-prod-api/monitoring", "monitoring deep link missing");
    expectStory(findTab(canvasElement, "monitoring")?.getAttribute("data-state") === "active", "monitoring tab should be active");
    expectStory(!normalizeText(canvasElement.textContent).includes("最近更新记录"), "monitoring should not render recent updates");
    expectStory(!findSectionCard(canvasElement, "auto-policy"), "monitoring should not render settings cards");
    expectStory(Boolean(monitorSummary), "monitoring section should retain the topbar monitor summary");
    expectStory(!normalizeText(monitorSummary?.textContent).includes("服务监控摘要"), "monitoring section should keep the compact topbar monitor summary without the subtitle");
    expectNoLegacyServiceDetailHero({ canvasElement, expectStory, context: "service detail deep links" });
    expectStory(!findSectionCard(canvasElement, "service-identifiers"), "monitoring should not render the overview-only identifiers card");
  },
};

export const UpdateHistorySection: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod", "svc-prod-api", "history", "更新与回滚历史统一按时间排序并可直达任务详情", { sidebarCollapsed: true }),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "update-history")));
    expectStory(canvasElement.querySelector(".appShell")?.classList.contains("appShellSidebarCollapsed"), "history evidence should render with the primary sidebar collapsed");
    expectStory(currentRoutePathname() === "/services/stack-prod/svc-prod-api/history", "history deep link missing");
    expectStory(findTab(canvasElement, "history")?.getAttribute("data-state") === "active", "history tab should be active");
    const headerLabels = Array.from(canvasElement.querySelectorAll<HTMLElement>('.serviceOperationHistoryHeader [role="columnheader"]')).map((cell) => normalizeText(cell.textContent));
    expectStory(JSON.stringify(headerLabels) === JSON.stringify(["记录", "状态", "备份", "来源", "时间", "操作"]), "history table should expose the backup column after status");
    await waitForCondition(() => canvasElement.querySelectorAll(".serviceOperationHistoryRow").length === 5);
    const rows = Array.from(canvasElement.querySelectorAll<HTMLElement>(".serviceOperationHistoryRow"));
    expectStory(rows.length === 5, "history should include all matching update and rollback jobs only");
    expectStory(normalizeText(rows[0]?.textContent).includes("job-all-api-5-2-4"), "history should sort newest jobs first");
    expectStory(rows.some((row) => normalizeText(row.textContent).includes("回滚") && normalizeText(row.textContent).includes("已回滚")), "rollback record should be rendered in the shared table");
    expectHistoryColumnsAligned(canvasElement);
    const failedRow = rows.find((row) => normalizeText(row.textContent).includes("job-stack-prod-batch"));
    expectStory(failedRow?.classList.contains("serviceOperationHistoryRowFailed"), "failed history row should be visually de-emphasized");
    expectStory(getComputedStyle(failedRow?.querySelector(".serviceOperationHistoryStatus") ?? canvasElement).opacity === "1", "failed history status must retain full visual prominence");
    expectStory(Array.from(rows).every((row) => row.querySelectorAll(".serviceOperationHistoryOperation > *").length === 2), "history operation content must stay within two visible text rows");
    expectStory(!["更新完成", "回滚完成", "任务执行失败"].some((summary) => normalizeText(canvasElement.textContent).includes(summary)), "history must omit summaries already expressed by operation type or status");
    expectStory(!normalizeText(canvasElement.textContent).includes("job-unrelated-web"), "unrelated service job must stay filtered");
    const backupSummaryRow = findHistoryRowByJobId(canvasElement, "job-auto-policy-api-5-2-3");
    expectStory(backupSummaryRow?.querySelector('.serviceOperationHistoryBackup')?.getAttribute("data-backup-state") === "ready", "matched backup row should render a ready backup summary");
    expectStory(normalizeText(backupSummaryRow?.querySelector(".serviceOperationHistoryBackup")?.textContent) === "2 个目标17.6 MiB", "matched backup row should show target count and aggregated source size");
    const emptyBackupRow = findHistoryRowByJobId(canvasElement, "job-all-api-5-2-4");
    expectStory(emptyBackupRow?.querySelector('.serviceOperationHistoryBackup')?.getAttribute("data-backup-state") === "empty", "rows without backup records should render the empty placeholder state");
    expectStory(normalizeText(emptyBackupRow?.querySelector(".serviceOperationHistoryBackup")?.textContent) === "-- --", "rows without backup records should stay neutral");

    rows[4]?.click();
    await waitForCondition(() => currentRoutePathname() === "/queue/job-stack-prod-batch");

    window.location.hash = "#/services/stack-prod/svc-prod-api/history";
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "update-history")));
    const rowsAfterClick = Array.from(canvasElement.querySelectorAll<HTMLElement>(".serviceOperationHistoryRow"));
    rowsAfterClick[0]?.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await waitForCondition(() => currentRoutePathname() === "/queue/job-all-api-5-2-4");

    window.location.hash = "#/services/stack-prod/svc-prod-api/history";
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "update-history")));
    const rowsAfterEnter = Array.from(canvasElement.querySelectorAll<HTMLElement>(".serviceOperationHistoryRow"));
    rowsAfterEnter[1]?.dispatchEvent(new KeyboardEvent("keydown", { key: " ", bubbles: true }));
    await waitForCondition(() => currentRoutePathname() === "/queue/job-rollback-api-5-2-2");
  },
};

export const UpdateHistorySectionEvidence: Story = {
  parameters: { dockrevApiScenario: "service-detail-history-rollback-action" },
  render: render("stack-prod", "svc-prod-api", "history", "更新与回滚历史统一按时间排序并可直达任务详情", { sidebarCollapsed: true }),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "update-history")));
    await waitForCondition(() => canvasElement.querySelectorAll(".serviceOperationHistoryRow").length === 5);
    expectStory(canvasElement.querySelector(".appShell")?.classList.contains("appShellSidebarCollapsed"), "history evidence should render with the primary sidebar collapsed");
    expectStory(findTab(canvasElement, "history")?.getAttribute("data-state") === "active", "history tab should be active");
    expectStory(Boolean(canvasElement.querySelector('[data-service-operation-action="rollback"]')), "history evidence should expose the current rollback action");
    expectHistoryColumnsAligned(canvasElement);
    expectStory(canvasElement.querySelector(".serviceOperationHistoryRowFailed")?.textContent?.includes("job-stack-prod-batch"), "history evidence should retain the de-emphasized failed row");
    expectStory(getComputedStyle(canvasElement.querySelector(".serviceOperationHistoryRowFailed .serviceOperationHistoryStatus") ?? canvasElement).opacity === "1", "history evidence must retain failed status prominence");
    expectStory(
      Array.from(canvasElement.querySelectorAll(".serviceOperationHistoryRow")).every((row) => row.querySelectorAll(".serviceOperationHistoryOperation > *").length === 2),
      "history evidence must keep operation content to two visible text rows",
    );
    expectStory(!["更新完成", "回滚完成", "任务执行失败"].some((summary) => normalizeText(canvasElement.textContent).includes(summary)), "history evidence must omit summaries already expressed by operation type or status");
  },
};

export const UpdateHistoryPagination: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo", dockrevJobsOverride: paginatedHistoryJobs },
  render: render("stack-prod", "svc-prod-api", "history", "完整更新历史以页面形式稳定浏览。", { sidebarCollapsed: true }),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => canvasElement.querySelectorAll(".serviceOperationHistoryRow").length === 20);
    const pageStatus = canvasElement.querySelector(".serviceOperationHistoryPagerStatus");
    const previous = canvasElement.querySelector<HTMLButtonElement>('button[aria-label="上一页"]');
    const next = canvasElement.querySelector<HTMLButtonElement>('button[aria-label="下一页"]');

    expectStory(normalizeText(pageStatus?.textContent) === "第 1 页，每页 20 条", "history pager should describe the first cursor page");
    expectStory(previous?.disabled, "previous page should be disabled on the first page");
    expectStory(!next?.disabled, "next page should be enabled on the first page");

    next?.click();
    await waitForCondition(() => canvasElement.querySelectorAll(".serviceOperationHistoryRow").length === 3);
    expectStory(normalizeText(pageStatus?.textContent) === "第 2 页，每页 20 条", "history pager should advance to the final cursor page");
    expectStory(!previous?.disabled, "previous page should be enabled on the final page");
    expectStory(next?.disabled, "next page should be disabled on the final page");
    expectStory(normalizeText(canvasElement.querySelector(".serviceOperationHistoryRow")?.textContent).includes("job-history-page-3"), "final page should render only the remaining records");
  },
};

export const UpdateHistoryReleaseNotes: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevGitHubReleasesByServiceId: {
      "svc-prod-api": {
        authMode: "anonymous",
        repo: { fullName: "acme/api", htmlUrl: "https://github.com/acme/api" },
        items: historyReleaseNotes,
        locateByVersion: {
          "5.2.4": {
            status: "found",
            matchedTag: "5.2.4",
            indexWithinWindow: 2,
            absoluteIndex: 22,
          },
        },
      },
    },
  },
  render: render("stack-prod", "svc-prod-api", "history", "从更新记录定位到对应版本的发布日志。", { sidebarCollapsed: true }),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(canvasElement.querySelector('[data-service-operation-action="release-notes"][data-release-version="5.2.4"]')));
    const action = canvasElement.querySelector<HTMLButtonElement>('[data-service-operation-action="release-notes"][data-release-version="5.2.4"] button');
    expectStory(action, "history release notes action missing");
    action.click();

    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => doc.querySelector('[data-release-drawer="true"]')?.getAttribute("data-release-drawer") === "true");
    await waitForCondition(() => doc.querySelector('[data-release-tag="5.2.4"]')?.getAttribute("data-release-highlighted") === "true");
    const renderedReleaseRows = doc.querySelectorAll(".releaseDrawerVirtualRow").length;
    expectStory(renderedReleaseRows > 0 && renderedReleaseRows < historyReleaseNotes.length, "release drawer must stay virtualized");
    expectStory(window.location.search.includes("releaseVersion=5.2.4"), "drawer URL should retain the target version");
  },
};

export const UpdateHistoryRollbackAction: Story = {
  parameters: { dockrevApiScenario: "service-detail-history-rollback-action" },
  render: render("stack-prod", "svc-prod-api", "history", "当前可回滚目标只在来源更新记录行提供回滚操作。", { sidebarCollapsed: true }),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => canvasElement.querySelectorAll(".serviceOperationHistoryRow").length === 5);
    const action = canvasElement.querySelector<HTMLButtonElement>('[data-service-operation-action="rollback"]');
    expectStory(action, "history rollback action missing");
    expectStory(!action.disabled, "history rollback action should be enabled for the current target source");
    expectStory(normalizeText(action.closest(".serviceOperationHistoryRow")?.textContent).includes("job-auto-policy-api-5-2-3"), "history rollback action must stay on the current rollback target source job");

    action.click();
    await waitForCondition(() => doc.body.textContent?.includes("确认回滚服务 api？") ?? false);
    expectStory(currentRoutePathname() === "/services/stack-prod/svc-prod-api/history", "history action must not navigate to job detail");
    expectStory(doc.body.textContent?.includes("来源任务"), "rollback confirmation should retain source job details");
  },
};

export const UpdateHistoryEmpty: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo", dockrevJobsOverride: [] },
  render: render("stack-prod", "svc-prod-api", "history", "更新记录空态保持稳定可读"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "update-history")));
    expectStory(normalizeText(canvasElement.textContent).includes("当前服务暂无操作记录"), "history empty state missing");
  },
};

export const UpdateHistoryRealtimeRefresh: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevJobsEventsPayload: 'id: 701\nevent: job_event\ndata: {"jobId":"job-all-api-5-2-4"}\n\n',
  },
  render: render("stack-prod", "svc-prod-api", "history", "更新记录激活时复用全局 jobs SSE 刷新列表"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "update-history")));
    await waitForCondition(() => Number(globalThis.__DOCKREV_MOCK_DEBUG__?.jobsEventsCalls ?? 0) >= 1);
    await waitForCondition(() => Number(globalThis.__DOCKREV_MOCK_DEBUG__?.jobsListCalls ?? 0) >= 2);
  },
};

export const UpdateHistoryBackupSizeFallback: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceBackupRecordsById: {
      "svc-prod-api": partialHistoryBackupRecords,
    },
  },
  render: render("stack-prod", "svc-prod-api", "history", "备份摘要在缺少源目标体积时保留数量并回退到中性占位。", { sidebarCollapsed: true }),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "update-history")));
    const backupSummaryRow = findHistoryRowByJobId(canvasElement, "job-auto-policy-api-5-2-3");
    const backupCell = backupSummaryRow?.querySelector<HTMLElement>(".serviceOperationHistoryBackup");
    expectStory(backupCell?.getAttribute("data-backup-state") === "partial", "missing source sizes should switch the backup summary into partial mode");
    expectStory(normalizeText(backupCell?.textContent) === "2 个目标--", "partial backup summary should keep the target count and neutralize the total size");
  },
};

export const SettingsSection: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod", "svc-prod-api", "settings", "设置子页集中自动更新、Compose、保护项与维护动作"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "auto-policy")));
    expectStory(currentRoutePathname() === "/services/stack-prod/svc-prod-api/settings", "settings deep link missing");
    expectStory(findTab(canvasElement, "settings")?.getAttribute("data-state") === "active", "settings tab should be active");
    expectStory(Boolean(findSectionCard(canvasElement, "auto-policy")), "settings should render auto policy card");
    expectStory(Boolean(findSectionCard(canvasElement, "ignore-rules")), "settings should render ignore rules");
    expectStory(Boolean(findSectionCard(canvasElement, "danger-zone")), "settings should render maintenance actions");
    expectStory(!normalizeText(canvasElement.textContent).includes("最近更新记录"), "settings should not render overview card");
  },
};

export const BackupSection: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod", "svc-prod-api", "backup", "备份子页集中备份摘要、记录列表与设置入口"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "backup-summary")));
    const banner = canvasElement.querySelector<HTMLElement>('[data-service-detail-context="status-summary"]');
    const tabsShell = canvasElement.querySelector<HTMLElement>('[data-service-detail-tabs-shell="true"]');
    expectStory(currentRoutePathname() === "/services/stack-prod/svc-prod-api/backup", "backup deep link missing");
    expectStory(findTab(canvasElement, "backup")?.getAttribute("data-state") === "active", "backup tab should be active");
    expectStory(Boolean(banner), "service-level status summary missing");
    expectStory(Boolean(tabsShell), "tab shell missing");
    expectStory(Boolean(banner && tabsShell && (banner.compareDocumentPosition(tabsShell) & Node.DOCUMENT_POSITION_FOLLOWING) === Node.DOCUMENT_POSITION_FOLLOWING), "service-level status summary should render before the tab shell");
    expectStory(Boolean(findSectionCard(canvasElement, "backup-records")), "backup should render backup records card");
    expectStory(normalizeText(canvasElement.textContent).includes("实际备份记录"), "backup records heading missing");
    expectStory(normalizeText(canvasElement.textContent).includes("备份时间"), "backup record card content missing");
  },
};

export const BackupRecordsActualOnly: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod", "svc-prod-api", "backup", "这里只展示实际产生过备份产物的记录；没有备份产物的尝试不会出现在这里。"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "backup-records")));
    expectStory(normalizeText(canvasElement.textContent).includes("这里只展示实际产生过备份产物的记录"), "actual-records subtitle missing");
    expectStory(normalizeText(canvasElement.textContent).includes("实际备份记录"), "actual-records heading missing");
    expectStory(normalizeText(canvasElement.textContent).includes("备份时间"), "actual-records content missing");
    expectStory(!normalizeText(canvasElement.textContent).includes("已跳过"), "skipped records should not be visible");
    expectStory(!normalizeText(canvasElement.textContent).includes("archive failed"), "failed attempt details should not be visible");
  },
};

export const LogsSection: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod", "svc-prod-api", "logs", "日志子页提供单服务实时日志、搜索与吸底"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes("实时日志"));
    expectStory(currentRoutePathname() === "/services/stack-prod/svc-prod-api/logs", "logs deep link missing");
    expectStory(findTab(canvasElement, "logs")?.getAttribute("data-state") === "active", "logs tab should be active");
    expectStory(normalizeText(canvasElement.textContent).includes("boot complete"), "logs should render stream lines");
    expectStory(normalizeText(canvasElement.textContent).includes("runtime perf"), "logs should render structured message text");
    expectStory(normalizeText(canvasElement.textContent).includes("admin_read"), "logs should render structured metadata chips");
    const tracingRow = findLogRowContaining(canvasElement, "openai proxy request started");
    expectStory(tracingRow, "logs should render parsed tracing text message");
    expectStory(tracingRow?.getAttribute("data-format") === "text", "tracing text row should stay text-formatted");
    expectStory(tracingRow?.getAttribute("data-level") === "info", "tracing text row should expose parsed info level");
    expectStory(normalizeText(tracingRow?.querySelector(".serviceLogLevel")?.textContent) === "INFO", "tracing text row should show parsed level badge");
    expectStory(!normalizeText(tracingRow?.querySelector(".serviceLogHumanMsg")?.textContent).includes("2026-07-07T05:54:01"), "human tracing message should omit the application timestamp prefix");
    expectStory(normalizeText(tracingRow?.textContent).includes("proxy_request_id2722"), "tracing text row should render parsed metadata chips");
    expectStory(normalizeText(canvasElement.textContent).includes("2026-06-29"), "logs should render the log date");
    expectStory(normalizeText(canvasElement.textContent).includes("ERR"), "logs should render inferred log levels");
    const input = canvasElement.querySelector<HTMLInputElement>('input[aria-label="搜索日志"]');
    expectStory(input, "logs search input missing");
    expectStory(Boolean(findButton(canvasElement, "Human")), "logs human toggle missing");
    expectStory(Boolean(findButton(canvasElement, "Raw")), "logs raw toggle missing");
    expectStory(canvasElement.querySelector('[data-service-logs-virtualized="true"]')?.getAttribute("data-service-logs-view") === "human", "logs should default to human mode");
    expectStory(Boolean(findButton(canvasElement, "自动换行 关")), "logs wrap toggle missing");
    expectStory(Boolean(findButton(canvasElement, "UTC")), "logs timezone toggle missing");
    expectStory(canvasElement.querySelector('[data-service-logs-virtualized="true"]')?.getAttribute("data-service-logs-wrap") === "off", "logs should default to nowrap mode");
    findButton(canvasElement, "Raw")?.click();
    await waitForCondition(() => canvasElement.querySelector('[data-service-logs-virtualized="true"]')?.getAttribute("data-service-logs-view") === "raw");
    expectStory(normalizeText(canvasElement.textContent).includes('"timestamp"'), "raw mode should expose original JSON text");
    expectStory(normalizeText(canvasElement.textContent).includes("2026-07-07T05:54:01.126674Z INFO openai proxy request started"), "raw mode should expose original tracing text with application timestamp and level");
    findButton(canvasElement, "Human")?.click();
    await waitForCondition(() => canvasElement.querySelector('[data-service-logs-virtualized="true"]')?.getAttribute("data-service-logs-view") === "human");
    input.value = "slow query";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.dispatchEvent(new Event("change", { bubbles: true }));
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes("1 /"));
    input.value = "freshness_probe";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.dispatchEvent(new Event("change", { bubbles: true }));
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes("runtime perf"));
  },
};

export const SettingsOfflineReadonly: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    pwaStatus: { isOnline: false },
  },
  render: render("stack-prod", "svc-prod-api", "settings", "离线时设置页应明确阻断，不伪装成本地可编辑"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes("设置页需要联网"));
    expectStory(currentRoutePathname() === "/services/stack-prod/svc-prod-api/settings", "offline settings deep link missing");
    expectStory(normalizeText(canvasElement.textContent).includes("当前离线"), "offline readonly banner missing");
    expectStory(normalizeText(canvasElement.textContent).includes("设置页包含敏感配置与写操作"), "settings offline gate detail missing");
    expectStory(!findSectionCard(canvasElement, "auto-policy"), "offline settings should not render editable cards");
  },
};

export const MobileSettingsOfflineReadonly: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    pwaStatus: { isOnline: false },
    viewport: { defaultViewport: "mobile1" },
  },
  render: render("stack-prod", "svc-prod-api", "settings", "移动端离线详情使用统一服务操作入口"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes("设置页需要联网"));
    const doc = canvasElement.ownerDocument;
    const menuButton = doc.querySelector<HTMLButtonElement>(".serviceMobileActionMenuTrigger");
    expectStory(menuButton, "offline mobile detail should expose the unified service action menu");
    expectStory(
      doc.querySelectorAll<HTMLElement>(".topActions > .serviceStackDetailAction, .topActions > .btn").length === 0,
      "offline mobile detail should not expose standalone header actions",
    );
    menuButton?.click();
    await waitForCondition(() => normalizeText(doc.body.textContent).includes("Stack 详情"));
    expectStory(
      doc.querySelector('[data-service-mobile-action-item="refresh"]')?.getAttribute("aria-disabled") === "true",
      "offline refresh should remain visible and disabled",
    );
    expectStory(Boolean(doc.querySelector('[data-service-mobile-action-item="stack-detail"]')), "stack detail action missing");
  },
};

export const MobileLogsSection: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    viewport: { defaultViewport: "mobile1" },
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

    const siblingLink = findLink(doc, "web");
    expectStory(siblingLink, "mobile service drawer should include sibling services");
    siblingLink.click();
    await waitForCondition(() => currentRoutePathname() === "/services/stack-prod/svc-prod-web/logs");
  },
};

export const MobileHistorySection: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    viewport: { defaultViewport: "mobile1" },
  },
  render: render("stack-prod", "svc-prod-api", "history", "移动端更新记录保留两行栅格且不产生横向滚动。"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => canvasElement.querySelectorAll(".serviceOperationHistoryRow").length === 5);
    const monitorSummary = canvasElement.ownerDocument.querySelector<HTMLElement>('[data-service-detail-context="monitor-summary"]');
    const table = canvasElement.querySelector<HTMLElement>(".serviceOperationHistoryTable");
    const row = canvasElement.querySelector<HTMLElement>(".serviceOperationHistoryRow");
    const statusRail = canvasElement.querySelector<HTMLElement>('[data-service-detail-context="status-summary"]');
    const historyShell = canvasElement.querySelector<HTMLElement>(".serviceOperationHistory");
    const mobileStatus = row?.querySelector<HTMLElement>(".serviceOperationHistoryMobileStatus");
    const desktopStatus = row?.querySelector<HTMLElement>(".serviceOperationHistoryStatus");
    const title = row?.querySelector<HTMLElement>(".serviceOperationHistoryOperationTitle");
    const topbar = canvasElement.ownerDocument.querySelector<HTMLElement>(".topbar");
    const menuButton = canvasElement.ownerDocument.querySelector<HTMLElement>(".mobileMenuButton");
    const brandLogo = canvasElement.ownerDocument.querySelector<HTMLElement>(".topbarLeft .brandLogoThemeSwitch");
    const appShell = canvasElement.ownerDocument.querySelector<HTMLElement>(".appShell");
    expectStory(findTab(canvasElement, "history")?.getAttribute("data-state") === "active", "mobile history tab should stay active");
    expectStory(Boolean(table), "mobile history table missing");
    expectStory(Boolean(row), "mobile history row missing");
    expectStory(Math.abs((appShell ?? canvasElement).getBoundingClientRect().left) <= 1, "mobile detail shell should render edge-to-edge without Storybook canvas gutters");
    expectStory(getComputedStyle(topbar ?? canvasElement).borderBottomWidth === "0px", "mobile detail topbar should not draw an extra divider above the history tabs");
    expectStory(Boolean(monitorSummary), "mobile history should keep the topbar monitor summary");
    expectStory(!canvasElement.querySelector('[data-service-detail-context="monitor-summary"]'), "mobile history should not restore the monitor summary row in the body");
    expectStory(Boolean(statusRail), "mobile history should keep the shared status rail");
    expectStory(!normalizeText(monitorSummary?.textContent).includes("服务监控摘要"), "mobile history should not restore the monitor subtitle");
    expectStory(!statusRail?.querySelector(".svcDetailSummaryName"), "mobile history should not restore the duplicated service name in the status rail");
    expectNoLegacyServiceDetailHero({ canvasElement, expectStory, context: "mobile history" });
    expectMobileTopbarMonitorHidden({ monitorSummary, expectStory });
    expectStory((statusRail?.scrollWidth ?? 0) <= (statusRail?.clientWidth ?? 0) + 1, "mobile status rail should wrap instead of overflowing horizontally");
    expectStory(
      getComputedStyle(historyShell ?? canvasElement).borderTopWidth === "0px" &&
        getComputedStyle(historyShell ?? canvasElement).backgroundImage === "none" &&
        getComputedStyle(historyShell ?? canvasElement).boxShadow === "none",
      "mobile history section should not wrap record rows in an extra outer card shell",
    );
    expectStory(getComputedStyle(desktopStatus ?? canvasElement).display === "none", "mobile history should hide the dedicated status column cell");
    expectStory(Boolean(mobileStatus?.textContent?.trim()), "mobile history should render a status pill beside the card title");
    expectStory(
      (mobileStatus?.getBoundingClientRect().left ?? 0) > (title?.getBoundingClientRect().right ?? Number.MAX_SAFE_INTEGER) - 1,
      "mobile history status pill should sit to the right of the card title",
    );
    expectStory(
      Math.abs((menuButton?.getBoundingClientRect().top ?? 0) - (brandLogo?.getBoundingClientRect().top ?? 0)) <= 2,
      "mobile detail topbar should keep menu and brand on the same first-row baseline",
    );
    expectStory(row?.scrollWidth != null && row.clientWidth > 0 && row.scrollWidth <= row.clientWidth + 1, "mobile history rows should not overflow horizontally");
    expectStory(getComputedStyle(row ?? canvasElement).gridTemplateAreas.includes("backup source time"), "mobile history should use the compact two-row grid with the backup column");
  },
};

export const LogsSectionVirtualized: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceLogsByServiceId: {
      "svc-prod-api": {
        snapshot: buildLongLogsSnapshot("svc-prod-api"),
        eventsPayload: ": keep-alive\n\n",
      },
    },
  },
  render: render("stack-prod", "svc-prod-api", "logs", "日志子页在大缓冲下继续使用虚拟列表，并提供自动换行切换"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes("实时日志"));
    const terminal = canvasElement.querySelector<HTMLElement>('[data-service-logs-virtualized="true"]');
    expectStory(terminal, "virtualized logs terminal missing");
    const totalCount = Number(terminal?.getAttribute("data-service-logs-total-count") ?? "0");
    const visibleCount = Number(terminal?.getAttribute("data-service-logs-visible-count") ?? "0");
    expectStory(totalCount >= 1600, "virtualized story should expose a large in-memory buffer");
    expectStory(visibleCount > 0 && visibleCount < totalCount, "virtualized story should only render the visible window");
    expectStory(canvasElement.querySelectorAll(".serviceLogRow").length === visibleCount, "rendered row count should match the virtualized visible window");

    const wrapButton = findButton(canvasElement, "自动换行 关");
    expectStory(wrapButton, "wrap toggle missing in virtualized story");
    wrapButton.click();
    await waitForCondition(() => Boolean(findButton(canvasElement, "自动换行 开")));
    expectStory(terminal?.getAttribute("data-service-logs-wrap") === "on", "wrap toggle should update terminal wrap state");

    const utcButton = findButton(canvasElement, "UTC");
    expectStory(utcButton, "timezone toggle missing in virtualized story");
  },
};

export const LogsSectionMultilineGrouping: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceLogsByServiceId: {
      "svc-prod-api": {
        snapshot: buildMultilineLogsSnapshot("svc-prod-api"),
        eventsPayload: ": keep-alive\n\n",
      },
    },
  },
  render: render("stack-prod", "svc-prod-api", "logs", "多行应用错误保持为一条日志组"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes("database is locked"));
    const rows = canvasElement.querySelectorAll<HTMLElement>(".serviceLogRow");
    expectStory(rows.length === 2, "multiline snapshot should render two logical log rows");
    const firstRow = rows[0];
    expectStory(firstRow?.getAttribute("data-multiline") === "true", "error row should be marked multiline");
    expectStory(firstRow?.getAttribute("data-inline-level") === "true", "inline tracing level should suppress duplicate badge text");
    expectStory(normalizeText(firstRow?.querySelector(".serviceLogMsg")?.textContent).includes("Caused by:"), "multiline row should keep continuation text in the message column");
    expectStory(firstRow?.querySelector(".serviceLogLevel")?.classList.contains("serviceLogLevelInline"), "inline tracing level should render with the compact marker style in the level column");
    expectStory(normalizeText(firstRow?.querySelector(".serviceLogLevel")?.textContent) === "", "inline tracing level should not repeat the textual level badge");
  },
};

export const LogsSectionEvidence: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod", "svc-prod-api", "logs", "日志子页提供单服务实时日志、搜索与吸底"),
  play: async ({ canvasElement, step }) => {
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes("实时日志"));
    expectStory(currentRoutePathname() === "/services/stack-prod/svc-prod-api/logs", "logs deep link missing");
    expectStory(findTab(canvasElement, "logs")?.getAttribute("data-state") === "active", "logs tab should be active");
    expectStory(normalizeText(canvasElement.textContent).includes("runtime perf"), "logs evidence story should render structured summary");
    expectStory(normalizeText(canvasElement.textContent).includes("dashboard_overview_phase"), "logs evidence story should render structured metadata");
    expectStory(normalizeText(canvasElement.textContent).includes("openai proxy request started"), "logs evidence story should render tracing text summary");
    expectStory(
      !normalizeText(findLogRowContaining(canvasElement, "openai proxy request started")?.querySelector(".serviceLogHumanMsg")?.textContent).includes("2026-07-07T05:54:01"),
      "logs evidence story should omit tracing timestamp from the human message",
    );
    expectStory(normalizeText(canvasElement.textContent).includes("worker sync complete jobs=18 queue=critical"), "logs evidence story should render denser stream lines");
    expectStory(normalizeText(canvasElement.textContent).includes("WARN"), "logs evidence story should expose inferred warning level");
    const input = canvasElement.querySelector<HTMLInputElement>('input[aria-label="搜索日志"]');
    expectStory(input, "logs search input missing");
    expectStory(input?.value === "", "logs evidence story should stay in default non-filtered state");
    expectStory(Boolean(findButton(canvasElement, "Human")), "logs evidence story should expose human toggle");
    expectStory(Boolean(findButton(canvasElement, "Raw")), "logs evidence story should expose raw toggle");
    expectStory(Boolean(findButton(canvasElement, "自动换行 关")), "logs evidence story should expose wrap toggle");

    const assertAligned = () => {
      const headerCells = canvasElement.querySelectorAll<HTMLElement>(".serviceLogsTerminalHead > span");
      const firstRowCells = canvasElement.querySelectorAll<HTMLElement>(".serviceLogRow:first-of-type > span");
      expectStory(headerCells.length === 3, "logs header should render three columns");
      expectStory(firstRowCells.length === 3, "logs first row should render three columns");
      for (let index = 0; index < 3; index += 1) {
        const headerLeft = Math.round(headerCells[index]!.getBoundingClientRect().left);
        const rowLeft = Math.round(firstRowCells[index]!.getBoundingClientRect().left);
        expectNearlyEqual(rowLeft, headerLeft, 1, `logs column ${index + 1} should align between header and body`);
      }
    };

    await step("desktop columns stay aligned", async () => {
      globalThis.innerWidth = 1280;
      globalThis.dispatchEvent(new Event("resize"));
      await waitForCondition(() => canvasElement.querySelectorAll(".serviceLogRow:first-of-type > span").length === 3);
      assertAligned();
    });

    await step("mobile columns stay aligned", async () => {
      globalThis.innerWidth = 390;
      globalThis.dispatchEvent(new Event("resize"));
      await waitForCondition(() => canvasElement.querySelectorAll(".serviceLogRow:first-of-type > span").length === 3);
      assertAligned();
      globalThis.innerWidth = 1280;
      globalThis.dispatchEvent(new Event("resize"));
    });
  },
};

export const TabNavigation: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod", "svc-prod-api", "overview", "页头 Tabs 直接驱动 service section 路由"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => findTab(canvasElement, "overview") != null);

    findTab(canvasElement, "versions")?.click();
    await waitForCondition(() => currentRoutePathname() === "/services/stack-prod/svc-prod-api/versions");
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "versions")));
    expectStory(findTab(canvasElement, "versions")?.getAttribute("data-state") === "active", "versions tab active state missing after switch");

    findTab(canvasElement, "history")?.click();
    await waitForCondition(() => currentRoutePathname() === "/services/stack-prod/svc-prod-api/history");
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "update-history")));
    expectStory(findTab(canvasElement, "history")?.getAttribute("data-state") === "active", "history tab active state missing after switch");

    findTab(canvasElement, "monitoring")?.click();
    await waitForCondition(() => currentRoutePathname() === "/services/stack-prod/svc-prod-api/monitoring");
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes("资源监控"));
    expectStory(findTab(canvasElement, "monitoring")?.getAttribute("data-state") === "active", "monitoring tab active state missing after switch");

    findTab(canvasElement, "backup")?.click();
    await waitForCondition(() => currentRoutePathname() === "/services/stack-prod/svc-prod-api/backup");
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "backup-summary")));
    expectStory(findTab(canvasElement, "backup")?.getAttribute("data-state") === "active", "backup tab active state missing after switch");

    findTab(canvasElement, "logs")?.click();
    await waitForCondition(() => currentRoutePathname() === "/services/stack-prod/svc-prod-api/logs");
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes("实时日志"));
    expectStory(findTab(canvasElement, "logs")?.getAttribute("data-state") === "active", "logs tab active state missing after switch");

    findTab(canvasElement, "settings")?.click();
    await waitForCondition(() => currentRoutePathname() === "/services/stack-prod/svc-prod-api/settings");
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "auto-policy")));
    expectStory(findTab(canvasElement, "settings")?.getAttribute("data-state") === "active", "settings tab active state missing after switch");
  },
};

export const AutoPolicyInherited: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod", "svc-prod-api", "settings"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "auto-policy")));
    expectStory(normalizeText(canvasElement.textContent).includes("继承 Stack"), "service auto policy inherited summary missing");
    expectStory(findButton(canvasElement, "Stack 详情"), "stack detail top action missing");
  },
};

export const AutoPolicyOverrideDelayed: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceOverridesById: {
      "svc-prod-api": {
        settings: {
          autoRollback: true,
          backupTargets: { bindPaths: { "/var/lib/api/data": "inherit" }, volumeNames: {} },
          repoUrl: "https://codeberg.org/acme/api",
          autoUpdatePolicy: {
            mode: "override",
            enabled: true,
            rules: [
              {
                id: "svc-stable",
                name: "Service stable",
                enabled: true,
                matcher: { type: "glob", pattern: "5.2.*" },
                action: "delayed",
                delay: { minAgeSeconds: 10800, minVersionLag: 3 },
              },
            ],
          },
        },
      },
    },
  },
  render: render("stack-prod", "svc-prod-api", "settings"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes("Service stable"));
    expectStory(normalizeText(canvasElement.textContent).includes("延迟 3h"), "nonlinear time slider label missing");
    expectStory(normalizeText(canvasElement.textContent).includes("落后 3 个匹配版本"), "version lag copy missing");

    const settingsTrigger = findActionButton(doc, "open-auto-policy", "设置");
    expectStory(settingsTrigger, "service auto policy drawer trigger missing");
    settingsTrigger.click();
    await waitForCondition(() => drawerText(doc).includes("自动更新策略"));
    await waitForCondition(() => drawerText(doc).includes("Service stable"));
    expectStory(!drawerText(doc).includes("更新前备份 / 回滚"), "auto policy drawer must not include backup settings");
    expectStory(drawerText(doc).includes("Service stable"), "service policy editor missing in drawer");
    expectStory(drawerText(doc).includes("历史版本命中预览"), "history match preview missing");
    await waitForCondition(() => drawerText(doc).includes("命中"));
    expectStory(doc.querySelector('[data-settings-drawer-drag-zone="true"]'), "drawer drag zone missing");
    expectStory(doc.querySelector("[data-vaul-handle]"), "drawer handle missing");

    const timeSlider = doc.querySelector<HTMLInputElement>('input[type="range"][aria-label="时间"]');
    expectStory(timeSlider, "time slider missing");
    timeSlider.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    timeSlider.value = "4";
    timeSlider.dispatchEvent(new Event("input", { bubbles: true }));
    timeSlider.dispatchEvent(new Event("change", { bubbles: true }));
    await waitForCondition(() => drawerText(doc).includes("延迟 6h"));

    const ruleInput = doc.querySelector<HTMLInputElement>(".autoPolicyPattern input");
    expectStory(ruleInput, "policy rule input missing");
    ruleInput.focus();
    ruleInput.setSelectionRange(0, Math.min(2, ruleInput.value.length));
    expectStory(ruleInput.selectionStart === 0 && ruleInput.selectionEnd === Math.min(2, ruleInput.value.length), "rule input text selection blocked");
  },
};

export const AutoPolicyInvalidRegexPreview: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceOverridesById: {
      "svc-prod-api": {
        settings: {
          autoRollback: true,
          backupTargets: { bindPaths: { "/var/lib/api/data": "inherit" }, volumeNames: {} },
          repoUrl: null,
          autoUpdatePolicy: {
            mode: "override",
            enabled: true,
            rules: [
              {
                id: "bad-regex",
                name: "Broken regex",
                enabled: true,
                matcher: { type: "regex", pattern: "[" },
                action: "delayed",
                delay: { minAgeSeconds: 900, minVersionLag: 1 },
              },
            ],
          },
        },
      },
    },
  },
  render: render("stack-prod", "svc-prod-api", "settings"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes("Broken regex"));
    findActionButton(doc, "open-auto-policy", "设置")?.click();
    await waitForCondition(() => drawerText(doc).includes("不确定"));
    expectStory(drawerText(doc).includes("规则无法预览"), "invalid regex preview state missing");
  },
};

export const AutoPolicyEmptyHistoryPreview: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevDiscoveryTimelineByServiceId: {
      "svc-prod-api": { items: [] },
    },
  },
  render: render("stack-prod", "svc-prod-api", "settings"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "auto-policy")));
    findActionButton(doc, "open-auto-policy", "设置")?.click();
    await waitForCondition(() => drawerText(doc).includes("暂无历史版本记录"));
  },
};

export const AutoPolicyHistoryPreviewError: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevDiscoveryTimelineErrorServiceIds: ["svc-prod-api"],
  },
  render: render("stack-prod", "svc-prod-api", "settings"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "auto-policy")));
    findActionButton(doc, "open-auto-policy", "设置")?.click();
    await waitForCondition(() => drawerText(doc).includes("mock discovery timeline failed"));
  },
};

export const ComposeTagEditorSuggestions: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceTagSuggestionsById: {
      "svc-prod-api": [
        { tag: "5.3.0", lastUsedAt: "2026-05-05T14:20:00Z", source: "manual", useCount: 3 },
        { tag: "5.2.7", lastUsedAt: "2026-05-01T09:00:00Z", source: "update", useCount: 2 },
        { tag: "stable", lastUsedAt: "2026-04-25T18:30:00Z", source: "manual", useCount: 1 },
      ],
    },
  },
  render: render("stack-prod", "svc-prod-api", "settings"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => findButton(doc, "编辑 tag") != null);
    const tagTrigger = findButton(doc, "编辑 tag");
    expectStory(tagTrigger, "compose tag drawer trigger missing");
    tagTrigger.click();
    await waitForCondition(() => doc.body.textContent?.includes("部署 tag") ?? false);
    expectStory(!drawerText(doc).includes("更新前备份 / 回滚"), "compose tag drawer should not include service protection settings");
    const input = Array.from(doc.body.querySelectorAll<HTMLInputElement>("input")).find((item) => item.placeholder === "例如 5.2.3 或 stable");
    expectStory(input, "compose tag input missing");
    expectStory(Number(globalThis.__DOCKREV_MOCK_DEBUG__?.serviceTagSuggestionCalls ?? -1) === 0, "suggestions should be lazy");
    input.focus();
    await waitForCondition(() => doc.body.textContent?.includes("5.3.0") ?? false);
    expectStory(doc.body.textContent?.includes("2026"), "suggestion subtitle should include last used time");
    expectStory(!doc.body.textContent?.includes("次"), "suggestion subtitle should not show use count");
    expectStory(Number(globalThis.__DOCKREV_MOCK_DEBUG__?.serviceTagSuggestionCalls ?? -1) === 1, "suggestions should load once");
    input.value = "5.2";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    await waitForCondition(() => doc.body.textContent?.includes("5.2.7") ?? false);
    expectStory(!doc.body.textContent?.includes("5.3.0"), "autocomplete should filter non-matching tag suggestions");
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await waitForCondition(() => input.value === "5.2.7");
    input.blur();
    input.focus();
    await new Promise((resolve) => setTimeout(resolve, 80));
    expectStory(Number(globalThis.__DOCKREV_MOCK_DEBUG__?.serviceTagSuggestionCalls ?? -1) === 1, "suggestions should not reload");
  },
};

export const ComposeTagEditorSaveError: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod", "svc-prod-api", "settings"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => findButton(doc, "编辑 tag") != null);
    findButton(doc, "编辑 tag")?.click();
    await waitForCondition(() => doc.body.textContent?.includes("部署 tag") ?? false);
    const input = Array.from(doc.body.querySelectorAll<HTMLInputElement>("input")).find((item) => item.placeholder === "例如 5.2.3 或 stable");
    expectStory(input, "compose tag input missing");
    input.focus();
    input.value = "compose-error";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    findButton(doc, "保存 tag")?.click();
    await waitForCondition(() => doc.body.textContent?.includes("variable interpolation") ?? false);
  },
};

export const ComposeTagEditorMobileDrawer: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceTagSuggestionsById: {
      "svc-prod-api": [
        { tag: "5.3.0", lastUsedAt: "2026-05-05T14:20:00Z", source: "manual", useCount: 3 },
        { tag: "5.2.7", lastUsedAt: "2026-05-01T09:00:00Z", source: "update", useCount: 2 },
      ],
    },
    docs: {
      description: {
        story: "Capture this story with a narrow viewport to verify the bottom settings drawer tag editor.",
      },
    },
  },
  render: render("stack-prod", "svc-prod-api", "settings"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => findButton(doc, "编辑 tag") != null);
    findButton(doc, "编辑 tag")?.click();
    await waitForCondition(() => doc.body.textContent?.includes("部署 tag") ?? false);
    expectStory(!drawerText(doc).includes("更新前备份 / 回滚"), "compose tag drawer should not include service protection settings");
    const input = Array.from(doc.body.querySelectorAll<HTMLInputElement>("input")).find((item) => item.placeholder === "例如 5.2.3 或 stable");
    expectStory(input, "compose tag input missing");
    input.focus();
    await waitForCondition(() => doc.body.textContent?.includes("5.3.0") ?? false);
  },
};

export const ServiceProtectionBackupTargets: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod", "svc-prod-api", "backup"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => findButton(doc, "编辑备份设置") != null);
    findButton(doc, "编辑备份设置")?.click();
    await waitForCondition(() => drawerText(doc).includes("备份项（服务级）"));
    expectStory(drawerText(doc).includes("Volumes"), "volume section missing");
    expectStory(drawerText(doc).includes("Bind paths"), "bind path section missing");
    expectStory(drawerText(doc).includes("/srv/dockrev/backups"), "backup storage summary missing");
    expectStory(drawerText(doc).includes("gzip"), "backup compression copy missing");
    expectStory(drawerText(doc).includes("停机备份"), "stop-related policy missing");
    expectStory(drawerText(doc).includes("在线备份"), "live-backup policy missing");
  },
};

export const ServiceProtectionSharedTargetOff: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod", "svc-prod-api", "backup"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => findButton(doc, "编辑备份设置") != null);
    findButton(doc, "编辑备份设置")?.click();
    await waitForCondition(() => drawerText(doc).includes("/srv/app/../shared/assets"));
    expectStory(drawerText(doc).includes("关联 2 个服务"), "related service count missing");
    expectStory(drawerText(doc).includes("当前服务不会为这个 target 触发自动备份"), "disabled policy copy missing");
  },
};

export const ServiceProtectionEmptyBackupTargets: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod", "svc-prod-worker", "backup"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => findButton(doc, "编辑备份设置") != null);
    findButton(doc, "编辑备份设置")?.click();
    await waitForCondition(() => drawerText(doc).includes("当前服务在 Compose 中未发现可备份 volume 或 bind path"));
  },
};

export const ServiceProtectionStorageSummaryOnly: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod", "svc-prod-api", "backup"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => findButton(doc, "编辑备份设置") != null);
    findButton(doc, "编辑备份设置")?.click();
    await waitForCondition(() => drawerText(doc).includes("最近 1 份保留"));
    expectStory(drawerText(doc).includes(".tar.gz"), "artifact extension summary missing");
    expectStory(drawerText(doc).includes("稳定 1h 后清理"), "retention summary missing");
  },
};

export const BackupRecordsEmpty: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod", "svc-prod-worker", "backup"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "backup-records")));
    expectStory(normalizeText(canvasElement.textContent).includes("当前服务暂无实际备份记录"), "backup empty state missing");
  },
};

export const BackupRecordsNoiseFiltered: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceBackupRecordsById: {
      "svc-prod-api": {
        records: [],
      },
    },
  },
  render: render("stack-prod", "svc-prod-api", "backup", "相关历史里没有任何实际备份产物；后端过滤掉未产生产物的尝试后，这里应为空。"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "backup-records")));
    expectStory(normalizeText(canvasElement.textContent).includes("相关历史里没有任何实际备份产物"), "noise-filter subtitle missing");
    expectStory(normalizeText(canvasElement.textContent).includes("当前服务暂无实际备份记录"), "noise-filter empty state missing");
  },
};

export const BackupRecordsLegacyMissingAssets: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceBackupRecordsById: {
      "svc-prod-api": {
        records: [
          {
            backupId: "bkp_legacy",
            jobId: "job_legacy",
            scope: "service",
            status: "success",
            createdAt: "2026-06-28T18:15:24.960797189Z",
            finishedAt: "2026-06-28T18:15:24.960797189Z",
            artifactPath: "/srv/dockrev/backups/stack-prod/20260628-181524.tar.gz",
          },
        ],
      },
    },
  },
  render: render("stack-prod", "svc-prod-api", "backup", "旧版实际备份记录缺少 assets 字段时仍稳定渲染"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "backup-records")));
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes("未记录资产明细"));
    expectStory(findTab(canvasElement, "backup")?.getAttribute("data-state") === "active", "backup tab should stay active");
    expectStory(normalizeText(canvasElement.textContent).includes("成功"), "legacy success backup status missing");
  },
};

export const AutoPolicyDisabled: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceOverridesById: {
      "svc-prod-api": {
        settings: {
          autoRollback: true,
          backupTargets: { bindPaths: { "/var/lib/api/data": "inherit" }, volumeNames: {} },
          repoUrl: null,
          autoUpdatePolicy: {
            mode: "disabled",
            enabled: false,
            rules: [],
          },
        },
      },
    },
  },
  render: render("stack-prod", "svc-prod-api", "settings"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes("不会执行 Stack 级自动部署策略"));
  },
};

export const HydratedRunningUpdate: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo-hydrated-update" },
  render: render("stack-prod", "svc-prod-api", "overview"),
};

export const Hint: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-infra", "svc-infra-loki", "overview"),
};

export const ArchMismatch: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-infra", "svc-infra-prom", "overview"),
};

export const CrossTag: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-infra", "svc-infra-postgres", "overview"),
};

export const ResolvedTag: Story = {
  parameters: { dockrevApiScenario: "resolved-tag-demo" },
  render: render("stack-resolved", "svc-resolved-web", "overview"),
};

export const Blocked: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod", "svc-prod-worker", "overview"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => Boolean(findButton(doc, "更新")))
    expectStory(findButton(doc, "更新")?.disabled, "blocked service must disable the update primary action")
    doc.querySelector<HTMLButtonElement>('[aria-label="更新操作菜单"]')?.click()
    await waitForCondition(() => Boolean(doc.querySelector('[data-service-split-item="preview-update"]')))
    const preview = doc.querySelector<HTMLButtonElement>('[data-service-split-item="preview-update"]')
    expectStory(preview?.disabled, "blocked service must disable preview updates")
    expectStory(Boolean(preview?.querySelector('small')), "blocked update must explain why it is unavailable")
  },
};

export const NoCandidate: Story = {
  parameters: { dockrevApiScenario: "no-candidates" },
  render: render("stack-1", "svc-a", "overview"),
};

export const ComposeFallbacks: Story = {
  parameters: { dockrevApiScenario: "service-detail-compose-fallbacks" },
  render: render("stack-prod", "svc-prod-api", "settings"),
};

export const VersionAnomalyUpdatable: Story = {
  parameters: { dockrevApiScenario: "service-detail-version-anomaly" },
  render: render("stack-prod", "svc-prod-api", "overview"),
};

export const InferencePendingCandidateLoading: Story = {
  parameters: { dockrevApiScenario: "services-inference-pending-candidate-loading" },
  render: render("stack-inference-pending", "svc-inference-pending", "overview"),
};

export const ResourceMonitorDisabled: Story = {
  parameters: { dockrevApiScenario: "service-detail-resource-monitor-disabled" },
  render: render("stack-prod", "svc-prod-api", "monitoring"),
};

export const ResourceMonitorEmpty: Story = {
  parameters: { dockrevApiScenario: "service-detail-resource-monitor-empty" },
  render: render("stack-prod", "svc-prod-api", "monitoring"),
};

export const ResourceMonitorStreamError: Story = {
  parameters: { dockrevApiScenario: "service-detail-resource-monitor-stream-error" },
  render: render("stack-prod", "svc-prod-api", "monitoring"),
};

export const RollbackAvailable: Story = {
  parameters: { dockrevApiScenario: "service-detail-rollback-available" },
  render: render("stack-prod", "svc-prod-api", "overview"),
};

export const RollbackUnavailable: Story = {
  parameters: { dockrevApiScenario: "service-detail-rollback-unavailable" },
  render: render("stack-prod", "svc-prod-api", "overview"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    const toggle = doc.querySelector<HTMLButtonElement>('[aria-label="更新操作菜单"]');
    toggle?.click();
    await waitForCondition(() => Boolean(doc.querySelector('[data-service-split-item="rollback"]')));

    const trigger = doc.querySelector<HTMLButtonElement>('[data-service-split-item="rollback"]');
    expectStory(trigger, "rollback action missing");
    expectStory(trigger.disabled, "rollback action should be disabled when no target is available");
    expectStory(normalizeText(trigger.textContent).includes("未找到可回滚到升级前版本的成功升级记录"), "rollback disabled reason missing");
  },
};

export const RollbackActive: Story = {
  parameters: { dockrevApiScenario: "service-detail-rollback-active" },
  render: render("stack-prod", "svc-prod-api", "overview"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    doc.querySelector<HTMLButtonElement>('[aria-label="更新操作菜单"]')?.click();
    await waitForCondition(() => Boolean(doc.querySelector('[data-service-split-item="rollback"]')));

    const trigger = doc.querySelector<HTMLButtonElement>('[data-service-split-item="rollback"]');
    expectStory(trigger, "active rollback action missing");
    expectStory(normalizeText(trigger.textContent).includes("回滚中…"), "active rollback label missing");
    trigger.click();

    await waitForCondition(() => window.location.hash.includes("/queue/job-rollback-service"));
  },
};

export const RollbackRefreshRaceAfterUpdate: Story = {
  parameters: { dockrevApiScenario: "service-detail-rollback-stale-after-update" },
  render: render("stack-prod", "svc-prod-api", "overview"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => findButton(doc, "更新") != null);

    const updateTrigger = findButton(doc, "更新");
    expectStory(updateTrigger, "service update action missing");
    updateTrigger.click();

    await waitForCondition(() => doc.body.textContent?.includes("确认更新服务 api？") ?? false);
    const confirmButtons = findButtons(doc.body, "更新").filter((button) => !button.disabled);
    const confirmTrigger = confirmButtons.at(-1) ?? null;
    expectStory(confirmTrigger, "service update confirm action missing");
    confirmTrigger.click();

    const toggle = doc.querySelector<HTMLButtonElement>('[aria-label="更新操作菜单"]');
    toggle?.click();
    await waitForCondition(() => normalizeText(doc.querySelector('[data-service-split-item="rollback"]')?.textContent).includes("回滚信息刷新中…"), 8_000);
    const refreshingRollback = doc.querySelector<HTMLButtonElement>('[data-service-split-item="rollback"]');
    expectStory(refreshingRollback, "rollback refresh state missing during update settlement");
    expectStory(refreshingRollback.disabled, "rollback refresh state should stay disabled");
    expectStory(normalizeText(refreshingRollback.textContent).includes("回滚信息刷新中…"), "rollback refresh hint should hide stale unavailable reason");

    await waitForCondition(() => {
      const rollback = findButton(doc, "回滚");
      return Boolean(rollback && !rollback.disabled && rollback.getAttribute("aria-busy") !== "true");
    }, 8_000);

    const rollback = findButton(doc, "回滚");
    expectStory(rollback, "rollback action missing after update settlement");
    expectStory(!rollback.disabled, "rollback action should recover to enabled state after refresh settles");
    expectStory(!normalizeText(rollback.textContent).includes("未找到可回滚到升级前版本的成功升级记录"), "rollback action should never restore stale unavailable history hint");
  },
};
