import type { Meta, StoryObj } from "@storybook/react";
import { userEvent, within } from "storybook/test";
import { StackDetailPage } from "../../pages/StackDetailPage";
import { currentRoutePathname } from "../../routes";
import { PageHarness } from "../mocks/PageHarness";
import { withDockrevMockApi } from "../mocks/withDockrevMockApi";
import { assertRecentUpdateClickNavigation } from "./recentUpdateStoryAssertions";
import { expectStory, findButton, findLink, waitForCondition } from "./storyAssertions";

const meta: Meta<typeof StackDetailPage> = {
  title: "Pages/StackDetailPage",
  component: StackDetailPage,
  decorators: [withDockrevMockApi],
  tags: ["autodocs"],
};

export default meta;
type Story = StoryObj<typeof StackDetailPage>;

function findActiveNavLink(
  root: ParentNode,
  text: string,
): HTMLAnchorElement | null {
  return (
    Array.from(
      root.querySelectorAll<HTMLAnchorElement>(
        ".navItemActive, .mobileBottomNavItemActive",
      ),
    ).find((link) =>
      (link.textContent?.replace(/\s+/g, " ").trim() ?? "").includes(text),
    ) ?? null
  );
}

function drawerText(doc: Document): string {
  return (
    doc
      .querySelector(".settingsDrawerContent")
      ?.textContent?.replace(/\s+/g, " ")
      .trim() ?? ""
  );
}

function render(stackId: string): Story["render"] {
  return () => (
    <PageHarness route={{ name: "stack", stackId }}>
      {({ onTopActions, onLastScanHint }) => (
        <StackDetailPage
          stackId={stackId}
          onLastScanHint={onLastScanHint}
          onTopActions={onTopActions}
        />
      )}
    </PageHarness>
  );
}

export const PolicyEnabled: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(
      () => canvasElement.textContent?.includes("自动更新结果") ?? false,
    );
    await waitForCondition(() => Boolean(doc.querySelector(".detailSidebar")));
    expectStory(
      canvasElement.textContent?.includes("Stable semver"),
      "stack policy rule missing",
    );
    expectStory(
      canvasElement.textContent?.includes("延迟 1h"),
      "stack policy time slider label missing",
    );
    expectStory(
      canvasElement.textContent?.includes("落后 2 个匹配版本"),
      "stack policy version lag label missing",
    );
    expectStory(
      canvasElement.textContent?.includes("最近更新记录"),
      "stack recent update records missing",
    );

    const settingsTrigger = findButton(doc, "设置");
    expectStory(settingsTrigger, "stack settings drawer trigger missing");
    settingsTrigger.click();
    await waitForCondition(() => drawerText(doc).includes("自动更新策略"));
    expectStory(
      drawerText(doc).includes("Stable semver"),
      "stack policy editor missing in drawer",
    );
    expectStory(
      !drawerText(doc).includes("更新前备份 / 回滚"),
      "stack auto policy drawer must stay independent",
    );
    expectStory(
      findActiveNavLink(doc, "服务"),
      "stack detail route should keep the Services navigation item active",
    );
    expectStory(
      doc
        .querySelector(".detailRouteStackLinkActive")
        ?.textContent?.includes("prod"),
      "current stack should stay highlighted",
    );
  },
};

export const PolicyDisabled: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-infra"),
  play: async ({ canvasElement }) => {
    await waitForCondition(
      () => canvasElement.textContent?.includes("自动更新结果") ?? false,
    );
    expectStory(
      canvasElement.textContent?.includes("未启用"),
      "disabled stack policy state missing",
    );
  },
};

export const RecentUpdateNavigation: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod"),
  play: async ({ canvasElement }) => {
    await waitForCondition(
      () => canvasElement.textContent?.includes("最近更新记录") ?? false,
    );
    await assertRecentUpdateClickNavigation({
      canvasElement,
      jobIndex: 0,
      waitForCondition,
    });
  },
};

export const RecentUpdateNavigationEvidence: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: render("stack-prod"),
};

export const MobileNavigation: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    viewport: { defaultViewport: "mobile1" },
  },
  render: render("stack-prod"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(
      () => canvasElement.textContent?.includes("自动更新结果") ?? false,
    );
    expectStory(
      findActiveNavLink(doc, "服务"),
      "mobile stack route should keep the Services navigation item active",
    );
    const stackActions = doc.querySelector<HTMLButtonElement>('[aria-label="Stack 操作"]');
    expectStory(stackActions, "mobile Stack page should expose the Stack actions menu");
    stackActions.click();
    const actionMenu = within(doc.body);
    await waitForCondition(() => Boolean(actionMenu.queryByText("启动")));
    expectStory(Boolean(actionMenu.queryByText("停止")), "mobile Stack menu should include stop");
    expectStory(Boolean(actionMenu.queryByText("重启")), "mobile Stack menu should include restart");
    expectStory(Boolean(actionMenu.queryByText("返回服务")), "mobile Stack menu should include navigation");
    await userEvent.keyboard("{Escape}");
    const menuButton =
      doc.querySelector<HTMLButtonElement>(".mobileMenuButton");
    expectStory(
      menuButton,
      "mobile stack page should expose the service tree drawer trigger",
    );
    menuButton.click();
    await waitForCondition(() =>
      (doc.querySelector("#mobileDockrevMenu")?.textContent ?? "").includes(
        "服务导航",
      ),
    );

    const serviceLink = findLink(doc, "api");
    expectStory(
      serviceLink,
      "mobile stack drawer should include stack services",
    );
    serviceLink.click();
    await waitForCondition(
      () => currentRoutePathname() === "/services/stack-prod/svc-prod-api",
    );
  },
};

export const LifecycleRunning: Story = {
  parameters: { dockrevApiScenario: "stack-detail-lifecycle-running" },
  render: render("stack-prod"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => Boolean(doc.querySelector('[data-service-split-action="Stack 生命周期"]')));
    const action = doc.querySelector('[data-service-split-action="Stack 生命周期"]')!;
    expectStory(action.textContent?.includes("停止"), "running Stack should default to stop");
    const menuTrigger = doc.querySelector<HTMLButtonElement>('[aria-label="Stack 生命周期菜单"]');
    expectStory(menuTrigger, "Stack lifecycle split menu should be discoverable");
    menuTrigger.click();
    const body = within(doc.body);
    await waitForCondition(() => Boolean(body.queryByText("启动")));
    expectStory(Boolean(body.queryByText("停止")), "Stack lifecycle menu should include stop");
    expectStory(Boolean(body.queryByText("重启")), "Stack lifecycle menu should include restart");
  },
};

export const LifecycleStopped: Story = {
  parameters: { dockrevApiScenario: "stack-detail-lifecycle-stopped" },
  render: render("stack-prod"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => Boolean(doc.querySelector('[data-service-split-action="Stack 生命周期"]')));
    expectStory(doc.querySelector('[data-service-split-action="Stack 生命周期"]')?.textContent?.includes("启动"), "stopped Stack should default to start");
  },
};

export const LifecycleUnavailable: Story = {
  parameters: { dockrevApiScenario: "stack-detail-lifecycle-partial" },
  render: render("stack-prod"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => Boolean(doc.querySelector('[data-service-split-action="Stack 生命周期"]')));
    const action = doc.querySelector('[data-service-split-action="Stack 生命周期"]')!;
    expectStory(Boolean(action.querySelector('[aria-disabled="true"]')), "partial Stack lifecycle should be disabled");
    expectStory(Boolean(doc.querySelector('[aria-label*="Stack 生命周期：Stack 内服务运行状态不一致"]')), "partial reason should be exposed");
  },
};

export const LifecycleActiveJob: Story = {
  parameters: { dockrevApiScenario: "stack-detail-lifecycle-active" },
  render: render("stack-prod"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => Boolean(doc.querySelector('[data-service-split-action="Stack 生命周期"]')));
    const action = doc.querySelector('[data-service-split-action="Stack 生命周期"]');
    expectStory(action?.textContent?.includes("操作进行中…"), "active Stack job should remain visible in the top action");
    const primary = action?.querySelector<HTMLButtonElement>('button');
    expectStory(primary && !primary.disabled, "active Stack job should be clickable");
    await new Promise((resolve) => setTimeout(resolve, 1400));
    expectStory(doc.querySelector('[data-service-split-action="Stack 生命周期"]') === action, "active lifecycle polling should preserve the top action tree");
  },
};

export const LifecycleStopConfirmation: Story = {
  parameters: { dockrevApiScenario: "stack-detail-lifecycle-running" },
  render: render("stack-prod"),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => Boolean(doc.querySelector('[data-service-split-action="Stack 生命周期"]')));
    const action = doc.querySelector('[data-service-split-action="Stack 生命周期"]')!;
    const menuTrigger = action.querySelector<HTMLButtonElement>('[aria-label="Stack 生命周期菜单"]')!;
    menuTrigger.click();
    const body = within(doc.body);
    await waitForCondition(() => Boolean(body.queryByRole("menuitem", { name: "停止" })));
    await userEvent.click(body.getByRole("menuitem", { name: "停止" }));
    await waitForCondition(() => Boolean(body.queryByText("确认停止 Stack prod？")));
    expectStory(body.getByText("该操作会立即影响 Stack 内的 3 个服务。"), "confirmation should include Stack service count");
    await userEvent.click(body.getByText("取消"));
    await new Promise((resolve) => setTimeout(resolve, 80));
    expectStory(!globalThis.__DOCKREV_MOCK_DEBUG__?.lastLifecycleRequest, "cancelled Stack stop should not submit a request");

    menuTrigger.click();
    await waitForCondition(() => Boolean(body.queryByRole("menuitem", { name: "停止" })));
    await userEvent.click(body.getByRole("menuitem", { name: "停止" }));
    await waitForCondition(() => Boolean(body.queryByText("确认停止 Stack prod？")));
    await userEvent.click(body.getByRole("button", { name: "停止" }));
    await waitForCondition(() => globalThis.__DOCKREV_MOCK_DEBUG__?.lastLifecycleRequest?.kind === "stack");
    const request = globalThis.__DOCKREV_MOCK_DEBUG__?.lastLifecycleRequest as { kind: string; action: string } | null | undefined;
    expectStory(request?.action === "stop", "confirmed Stack stop should submit stop");
  },
};
