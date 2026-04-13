import type { Meta, StoryObj } from "@storybook/react";
import { ServicesPage } from "../../pages/ServicesPage";
import { PageHarness } from "../mocks/PageHarness";
import { withDockrevMockApi } from "../mocks/withDockrevMockApi";

const meta: Meta<typeof ServicesPage> = {
  title: "Pages/ServicesPage",
  component: ServicesPage,
  decorators: [withDockrevMockApi],
  tags: ["autodocs"],
};

export default meta;

type Story = StoryObj<typeof ServicesPage>;

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function expectStory(condition: unknown, message: string): asserts condition {
  if (!condition) throw new globalThis.Error(message);
}

function renderServices(pageSubtitle: string): Story["render"] {
  return () => (
    <PageHarness
      route={{ name: "services" }}
      title="服务"
      pageSubtitle={pageSubtitle}
    >
      {({ onLastScanHint, onTopActions }) => (
        <ServicesPage
          onLastScanHint={onLastScanHint}
          onTopActions={onTopActions}
        />
      )}
    </PageHarness>
  );
}

export const Default: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: renderServices(
    "运维大盘接管概览：运行态、发现异常、更新候选 + 归档恢复",
  ),
};

export const DashboardDemo: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: renderServices("兼容旧 smoke：运维大盘默认场景"),
};

export const GuideLineLongNames: Story = {
  parameters: { dockrevApiScenario: "guide-line-long-names" },
  render: renderServices("兼容旧 smoke：长名称分组对齐"),
};

export const HydratedRunningUpdate: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo-hydrated-update" },
  render: renderServices("兼容旧 smoke：运行中任务首屏注水"),
};

export const InferencePendingCandidateLoading: Story = {
  parameters: {
    dockrevApiScenario: "services-inference-pending-candidate-loading",
  },
  render: renderServices("兼容旧 smoke：候选版本推测加载态"),
};

export const VersionTagsPopoverDemo: Story = {
  parameters: { dockrevApiScenario: "version-tags-popover-demo" },
  render: renderServices("兼容旧 smoke：版本标签弹层本地刷新"),
};

export const DigestPinnedImageDisplay: Story = {
  parameters: { dockrevApiScenario: "digest-pinned-image-display" },
  render: renderServices("兼容旧 smoke：digest 固定镜像显示"),
};

export const CandidateSearchKeepsArchivedVisible: Story = {
  parameters: { dockrevApiScenario: "multi-stack-mixed" },
  render: renderServices("回归：更新候选搜索只过滤候选区，不影响归档恢复区"),
  play: async ({ canvasElement }) => {
    await sleep(260);

    expectStory(
      canvasElement.textContent?.includes("运行态与结果"),
      "expected operations dashboard jobs card on services page",
    );
    expectStory(
      canvasElement.textContent?.includes("扫描与发现异常"),
      "expected discovery card on services page",
    );
    expectStory(
      canvasElement.textContent?.includes("已归档"),
      "expected archived section on services page",
    );

    const searchInput = canvasElement.querySelector<HTMLInputElement>(
      'input[placeholder*="Homepage"]',
    );
    expectStory(
      searchInput,
      "expected candidate search input in operations dashboard",
    );
    searchInput.value = "Primary API";
    searchInput.dispatchEvent(new Event("input", { bubbles: true }));
    await sleep(160);

    const visibleRows = Array.from(
      canvasElement.querySelectorAll<HTMLElement>(".rowLine"),
    );
    expectStory(
      visibleRows.length >= 1,
      "candidate search should still leave matching candidate rows visible",
    );
    expectStory(
      visibleRows.every(
        (row) =>
          row.textContent?.includes("api") ||
          row.textContent?.includes("Acme API") ||
          row.textContent?.includes("Primary API"),
      ),
      "candidate search should filter the update candidate table by homepage name/description",
    );
    expectStory(
      canvasElement.textContent?.includes(
        "已归档 services（按所属 stack 聚合）",
      ),
      "archived services section should remain visible after candidate search",
    );

    const stackAction = canvasElement.querySelector<HTMLButtonElement>(
      ".tableGroup .groupHead .actionCell button",
    );
    expectStory(stackAction, "expected filtered stack action button");
    stackAction.click();
    await sleep(220);

    const dialog =
      document.querySelector<HTMLElement>('[role="alertdialog"]') ??
      document.querySelector<HTMLElement>('[role="dialog"]');
    expectStory(dialog, "expected filtered stack update confirm dialog");
    expectStory(
      dialog.textContent?.includes("stack（当前筛选）"),
      "confirm dialog should disclose filtered stack scope",
    );
    expectStory(
      dialog.textContent?.includes("1 / 3"),
      "confirm dialog should report visible services vs full stack services",
    );
    expectStory(
      dialog.textContent?.includes("候选服务") &&
        dialog.textContent?.includes("1 个（可更新/需确认）"),
      "confirm dialog should only count visible matching candidates",
    );
    expectStory(
      dialog.querySelectorAll(".modalListItem").length === 1,
      "filtered stack update preview should only include the visible matching service",
    );

    const cancelButton = Array.from(
      dialog.querySelectorAll<HTMLButtonElement>("button"),
    ).find((button) => button.textContent?.includes("取消"));
    cancelButton?.click();
    await sleep(120);
  },
};
