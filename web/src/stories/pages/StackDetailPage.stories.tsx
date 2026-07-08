import type { Meta, StoryObj } from "@storybook/react";
import { StackDetailPage } from "../../pages/StackDetailPage";
import { currentRoutePathname } from "../../routes";
import { PageHarness } from "../mocks/PageHarness";
import { withDockrevMockApi } from "../mocks/withDockrevMockApi";

const meta: Meta<typeof StackDetailPage> = {
  title: "Pages/StackDetailPage",
  component: StackDetailPage,
  decorators: [withDockrevMockApi],
  tags: ["autodocs"],
};

export default meta;
type Story = StoryObj<typeof StackDetailPage>;

function expectStory(condition: unknown, message: string): asserts condition {
  if (!condition) throw new globalThis.Error(message);
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitForCondition(
  check: () => boolean,
  timeoutMs = 3000,
): Promise<void> {
  const started = Date.now();
  while (!check()) {
    if (Date.now() - started > timeoutMs)
      throw new globalThis.Error("condition timeout");
    await sleep(60);
  }
}

function findButton(root: ParentNode, text: string): HTMLButtonElement | null {
  return (
    Array.from(root.querySelectorAll<HTMLButtonElement>("button")).find(
      (button) => button.textContent?.replace(/\s+/g, " ").trim() === text,
    ) ?? null
  );
}

function findLink(root: ParentNode, text: string): HTMLAnchorElement | null {
  return (
    Array.from(root.querySelectorAll<HTMLAnchorElement>("a")).find((link) =>
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
