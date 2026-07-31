import type { Meta } from "@storybook/react";
import { ServiceDetailPage } from "../../pages/ServiceDetailPage";
import { withDockrevMockApi } from "../mocks/withDockrevMockApi";
import { expectTopbarMonitorSummary } from "./serviceDetailHeaderAssertions";
import { findActionButton, render, type ServiceDetailStory } from "./serviceDetailStoryShared";
import { expectStory, findButton, normalizeText, waitForCondition } from "./storyAssertions";

const meta: Meta<typeof ServiceDetailPage> = {
  title: "Pages/ServiceDetailPage",
  component: ServiceDetailPage,
  decorators: [withDockrevMockApi],
  tags: ["autodocs"],
  parameters: { layout: "fullscreen" },
};

export default meta;
type Story = ServiceDetailStory;

export const UpdateConfirmOpen: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod", "svc-prod-api", "overview"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => findButton(doc, "更新") != null);
    const updateTrigger = findButton(doc, "更新");
    expectStory(updateTrigger, "service update action missing");
    updateTrigger.click();
    await waitForCondition(() => doc.body.textContent?.includes("确认更新服务 api？") ?? false);
    expectStory(doc.body.textContent?.includes("版本"), "service update confirm version summary missing");
    expectStory(doc.body.textContent?.includes("目标 digest"), "service update confirm target digest missing");
    expectStory(doc.body.textContent?.includes("架构策略"), "service update confirm arch policy missing");
  },
};

export const RollbackConfirmOpen: Story = {
  parameters: { dockrevApiScenario: "service-detail-rollback-confirm-open" },
  render: render("stack-prod", "svc-prod-api", "overview"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    doc.querySelector<HTMLButtonElement>('[aria-label="更新操作菜单"]')?.click();
    await waitForCondition(() => Boolean(doc.querySelector('[data-service-split-item="rollback"]')));
    const trigger = doc.querySelector<HTMLButtonElement>('[data-service-split-item="rollback"]');
    expectStory(trigger, "rollback action missing");
    trigger.click();
    await waitForCondition(() => doc.body.textContent?.includes("确认回滚服务 api？") ?? false);
    expectStory(doc.body.textContent?.includes("当前版本"), "rollback confirm current version missing");
    expectStory(doc.body.textContent?.includes("回滚目标"), "rollback confirm target version missing");
    expectStory(doc.body.textContent?.includes("来源任务"), "rollback confirm source job missing");
    expectStory(doc.body.textContent?.includes("来源备份"), "rollback confirm backup summary missing");
    expectStory(doc.body.textContent?.includes("2 个目标"), "rollback confirm backup target count missing");
    expectStory(doc.body.textContent?.includes("执行回滚"), "rollback confirm action missing");
  },
};

export const RepoLinkEditing: Story = {
  parameters: { dockrevApiScenario: "repo-link-editing" },
  render: render("stack-prod", "svc-prod-api", "settings"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => findActionButton(doc, "open-service-settings", "打开") != null);
    findActionButton(doc, "open-service-settings", "打开")?.click();
    await waitForCondition(() => doc.body.textContent?.includes("服务保护设置") ?? false);
    const helper = Array.from(doc.body.querySelectorAll<HTMLElement>(".muted")).find((node) => node.textContent?.includes("清空并保存会禁用后续自动补齐"));
    expectStory(helper, "repoUrl auto-backfill helper copy missing in service detail story");
  },
};

export const Error: Story = {
  parameters: { dockrevApiScenario: "error" },
  render: render("stack-prod", "svc-prod-api", "overview"),
};

export const LifecycleRunning: Story = {
  parameters: { dockrevApiScenario: "service-detail-lifecycle-running" },
  render: render("stack-prod", "svc-prod-api", "overview", "运行中的服务默认提供停止操作。"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => findButton(doc, "停止") != null);
    const monitorSummary = doc.querySelector<HTMLElement>('[data-service-detail-context="monitor-summary"]');
    expectStory(!findButton(doc, "停止")?.disabled, "running service should enable stop as the primary action");
    expectStory((findButton(doc, "更新")?.getBoundingClientRect().width ?? Number.POSITIVE_INFINITY) < 100, "split update action must not inherit the generic 132px topbar width");
    expectStory(normalizeText(doc.querySelector(".topbarRouteTitle")?.textContent) === "api", "AppShell topbar should show the current service name");
    expectStory(Boolean(monitorSummary), "running service should keep the monitor summary in the AppShell topbar");
    expectTopbarMonitorSummary({ monitorSummary, expectStory });
    expectStory(!doc.querySelector(".pageHead .h1"), "service detail body must not repeat the service name");
    const toggle = doc.querySelector<HTMLButtonElement>('[aria-label="服务生命周期菜单"]');
    expectStory(toggle, "lifecycle split menu toggle missing");
    toggle?.click();
    await waitForCondition(() => Boolean(doc.querySelector('[role="menu"][aria-label="服务生命周期"]')));
    expectStory(Boolean(doc.querySelector('[data-service-split-item="lifecycle-start"]')), "lifecycle menu must retain start");
    expectStory(Boolean(doc.querySelector('[data-service-split-item="lifecycle-restart"]')), "lifecycle menu must retain restart");
  },
};

export const LifecycleRunningMobile: Story = {
  globals: { viewport: { value: "dockrevMobile", isRotated: false } },
  parameters: { dockrevApiScenario: "service-detail-lifecycle-running" },
  render: render("stack-prod", "svc-prod-api", "overview", "移动端首行使用图标 Logo，并在右侧显示当前服务名。"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => Boolean(doc.querySelector('[aria-label="服务操作"]')));
    doc.querySelector<HTMLButtonElement>('[aria-label="服务操作"]')?.click();
    await waitForCondition(() => Boolean(doc.querySelector('[role="menu"][aria-label="服务操作"]')));
    expectStory(doc.querySelectorAll('[data-service-mobile-action-group]').length === 3, "mobile service menu should expose three flat groups");
    expectStory(doc.querySelectorAll('[data-service-mobile-action-separator]').length === 2, "mobile service menu should separate its three groups");
    expectStory(Boolean(doc.querySelector('[data-service-mobile-action-item="execute-update"]')), "mobile service menu should retain update");
    expectStory(Boolean(doc.querySelector('[data-service-mobile-action-item="lifecycle-stop"]')), "mobile service menu should retain lifecycle actions");
    expectStory(Boolean(doc.querySelector('[data-service-mobile-action-item="stack-details"]')), "mobile service menu should retain Stack details");
    expectStory(!doc.querySelector('[data-slot="dropdown-menu-sub-trigger"]'), "mobile service menu should not introduce nested menus");
  },
};

export const LifecycleStopped: Story = {
  parameters: { dockrevApiScenario: "service-detail-lifecycle-stopped" },
  render: render("stack-prod", "svc-prod-api", "overview", "停止的服务默认提供启动操作。"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => Boolean(findButton(doc, "启动")));
    expectStory(!findButton(doc, "启动")?.disabled, "stopped service should enable start as the primary action");
  },
};

export const LifecyclePartial: Story = {
  parameters: { dockrevApiScenario: "service-detail-lifecycle-partial" },
  render: render("stack-prod", "svc-prod-api", "overview", "部分副本运行时保留菜单但不允许继续操作。"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => Boolean(findButton(doc, "停止")));
    expectStory(findButton(doc, "停止")?.disabled, "partial lifecycle state must disable its primary action");
  },
};

export const LifecycleUnknown: Story = {
  globals: { viewport: { value: "dockrevMobile", isRotated: false } },
  parameters: { dockrevApiScenario: "service-detail-lifecycle-unknown" },
  render: render("stack-prod", "svc-prod-api", "overview", "运行态未知时只保留可发现的生命周期菜单。"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => Boolean(findButton(doc, "停止")));
    expectStory(findButton(doc, "停止")?.disabled, "unknown lifecycle state must disable its primary action");
    doc.querySelector<HTMLButtonElement>('[aria-label="服务操作"]')?.click();
    await waitForCondition(() => Boolean(doc.querySelector('[data-service-mobile-action-item="lifecycle-restart"]')));
    expectStory(doc.querySelector<HTMLButtonElement>('[data-service-mobile-action-item="lifecycle-restart"]')?.getAttribute("aria-disabled") === "true", "unknown lifecycle menu actions must stay unavailable");
  },
};

export const LifecycleActive: Story = {
  parameters: { dockrevApiScenario: "service-detail-lifecycle-active" },
  render: render("stack-prod", "svc-prod-api", "overview", "活动生命周期任务可直接进入详情。"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => Boolean(findButton(doc, "操作进行中…")));
    expectStory(!findButton(doc, "操作进行中…")?.disabled, "active lifecycle task should remain navigable");
  },
};

export const LifecycleStopConfirmOpen: Story = {
  parameters: { dockrevApiScenario: "service-detail-lifecycle-running" },
  render: render("stack-prod", "svc-prod-api", "overview"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => findButton(doc, "停止") != null);
    findButton(doc, "停止")?.click();
    await waitForCondition(() => doc.body.textContent?.includes("确认停止服务 api？") ?? false);
    expectStory(doc.body.textContent?.includes("停止"), "stop confirmation must appear before creating a task");
  },
};
