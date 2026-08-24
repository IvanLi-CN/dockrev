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

function setInputValue(input: HTMLInputElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(
    HTMLInputElement.prototype,
    "value",
  )?.set;
  setter?.call(input, value);
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(new Event("change", { bubbles: true }));
}

function findStackUpdateButton(
  canvasElement: HTMLElement,
): HTMLButtonElement | undefined {
  return Array.from(
    canvasElement.querySelectorAll<HTMLButtonElement>(
      ".tableGroup .groupHead .actionCell button",
    ),
  ).find((button) => button.textContent?.includes("更新此 stack"));
}

function assertNoStackPolicyButtons(canvasElement: HTMLElement) {
  const policyButtons = Array.from(
    canvasElement.querySelectorAll<HTMLButtonElement>(
      ".tableGroup .groupHead .actionCell button",
    ),
  ).filter((button) => button.textContent?.trim() === "策略");
  expectStory(
    policyButtons.length === 0,
    "services update candidate stack groups should not render policy buttons",
  );
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

export const DiscoveryStopped: Story = {
  parameters: { dockrevApiScenario: "overview-discovery-readable" },
  render: renderServices("停止项目保持可启动，不进入发现异常"),
  play: async ({ canvasElement }) => {
    await sleep(260);

    const stoppedSection = canvasElement.querySelector(".discoveryStoppedSection");
    expectStory(stoppedSection, "expected a separate stopped projects section");
    expectStory(
      stoppedSection?.textContent?.includes("已停止，可启动"),
      "expected stopped projects to be explicitly startable",
    );
    expectStory(
      stoppedSection?.querySelectorAll(".discoveryStoppedRow").length === 6,
      "expected stopped projects to be capped at six rows",
    );
    expectStory(
      stoppedSection?.textContent?.includes("共 7 项"),
      "expected the stopped project total to remain visible",
    );
    expectStory(
      canvasElement.querySelector(".discoveryStatChipTotal .discoveryStatValue")?.textContent === "4",
      "expected stopped projects to stay out of the issue total",
    );
    const firstStoppedProject = stoppedSection?.querySelector<HTMLButtonElement>(
      ".discoveryStoppedRow",
    );
    expectStory(firstStoppedProject && !firstStoppedProject.disabled, "expected stopped stack to link to its details");
    firstStoppedProject?.click();
    await sleep(80);
    expectStory(
      window.location.pathname.endsWith("/services/stack-prod"),
      "expected stopped stack row to navigate to the existing stack detail route",
    );
  },
};

export const StaleTempOverrideReconciliation: Story = {
  parameters: {
    dockrevApiScenario: "overview-discovery-stale-temp-reconcile",
    viewport: { defaultViewport: "dockrevWide" },
  },
  render: renderServices("历史临时 Compose override 保留告警，并提供显式修复"),
};

export const StaleTempOverrideConfirmation: Story = {
  parameters: {
    dockrevApiScenario: "overview-discovery-stale-temp-reconcile",
    viewport: { defaultViewport: "dockrevWide" },
  },
  render: renderServices("确认后仅重建受影响服务，不拉取镜像"),
  play: async ({ canvasElement }) => {
    await sleep(260);
    expectStory(canvasElement.textContent?.includes("扫描与发现异常"), "expected the discovery card");
    expectStory(canvasElement.textContent?.includes("file-storage"), "expected the affected project name");
    expectStory(canvasElement.textContent?.includes("config_files_stale_dockrev_temp_override"), "expected stale provenance warning details");
    const action = Array.from(canvasElement.querySelectorAll<HTMLButtonElement>("button")).find((button) => button.textContent?.trim() === "修复标签");
    expectStory(action, "expected an explicit reconciliation action");
    action.click();
    await sleep(180);
    const dialog = document.querySelector<HTMLElement>('[role="alertdialog"], [role="dialog"]');
    expectStory(dialog, "expected reconciliation confirmation dialog");
    expectStory(dialog.textContent?.includes("仅重建该告警关联的服务"), "confirmation should disclose affected services");
    expectStory(dialog.textContent?.includes("--pull never"), "confirmation should disclose the no-pull policy");
    expectStory(dialog.textContent?.includes("不拉取、不猜测标签"), "confirmation should disclose the image safety rule");
    const cancel = Array.from(dialog.querySelectorAll<HTMLButtonElement>("button")).find((button) => button.textContent?.includes("取消"));
    cancel?.click();
  },
};

export const AllActionableServicesVisible: Story = {
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: renderServices("回归：默认列表必须完整显示全部 actionable 服务"),
  play: async ({ canvasElement }) => {
    await sleep(260);

    const rows = Array.from(
      canvasElement.querySelectorAll<HTMLElement>(".rowLine"),
    );
    expectStory(
      rows.length === 6,
      `expected all 6 actionable services to be visible, received ${rows.length}`,
    );

    for (const label of [
      "api",
      "web",
      "worker",
      "loki",
      "prometheus",
      "postgres",
    ]) {
      expectStory(
        rows.some((row) => row.textContent?.includes(label)),
        `expected actionable row for ${label}`,
      );
    }

    expectStory(
      canvasElement.textContent?.includes("可更新 3"),
      "expected actionable filter count for updatable services",
    );
    expectStory(
      canvasElement.textContent?.includes("需确认 1"),
      "expected actionable filter count for hint services",
    );
    expectStory(
      canvasElement.textContent?.includes("架构不匹配 1"),
      "expected actionable filter count for arch mismatch services",
    );
    expectStory(
      canvasElement.textContent?.includes("被阻止 1"),
      "expected actionable filter count for blocked services",
    );
  },
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
    assertNoStackPolicyButtons(canvasElement);

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
    setInputValue(searchInput, "Primary API");
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

    expectStory(
      !canvasElement.textContent?.includes("仅更新当前筛选结果"),
      "inline aggregate scope hint should not be rendered on the page",
    );

    const allAction = Array.from(
      canvasElement.querySelectorAll<HTMLButtonElement>("button"),
    ).find((button) => button.textContent?.includes("更新全部"));
    expectStory(allAction, "expected top-level aggregate update button");
    allAction.click();
    await sleep(220);

    const allDialog =
      document.querySelector<HTMLElement>('[role="alertdialog"]') ??
      document.querySelector<HTMLElement>('[role="dialog"]');
    expectStory(allDialog, "expected aggregate-all confirm dialog");
    expectStory(
      allDialog.textContent?.includes("候选服务") &&
        allDialog.textContent?.includes("1 个（可更新/需确认）"),
      "aggregate-all confirm dialog should only submit currently visible candidates",
    );
    expectStory(
      allDialog.querySelectorAll(".modalListItem").length === 1,
      "aggregate-all preview should list only the currently visible submitted target",
    );

    const allCancelButton = Array.from(
      allDialog.querySelectorAll<HTMLButtonElement>("button"),
    ).find((button) => button.textContent?.includes("取消"));
    allCancelButton?.click();
    await sleep(120);

    const stackAction = findStackUpdateButton(canvasElement);
    expectStory(stackAction, "expected filtered stack action button");
    stackAction.click();
    await sleep(220);

    const dialog =
      document.querySelector<HTMLElement>('[role="alertdialog"]') ??
      document.querySelector<HTMLElement>('[role="dialog"]');
    expectStory(dialog, "expected filtered stack update confirm dialog");
    expectStory(
      dialog.textContent?.includes("范围") &&
        dialog.textContent?.includes("stack"),
      "confirm dialog should keep stack scope semantics explicit",
    );
    expectStory(
      dialog.textContent?.includes("候选服务") &&
        dialog.textContent?.includes("1 个（可更新/需确认）"),
      "confirm dialog should only submit the currently visible stack target",
    );
    expectStory(
      dialog.querySelectorAll(".modalListItem").length === 1,
      "filtered stack update preview should include only the visible stack target",
    );

    const cancelButton = Array.from(
      dialog.querySelectorAll<HTMLButtonElement>("button"),
    ).find((button) => button.textContent?.includes("取消"));
    cancelButton?.click();
    await sleep(120);
  },
};

export const SameTagDigestUpdateVisible: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceOverridesById: {
      "svc-prod-api": {
        image: {
          ref: "ghcr.io/acme/api:latest",
          tag: "latest",
          digest:
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          resolvedTag: "v5.2.3",
          resolvedTags: ["v5.2.3"],
        },
        candidate: {
          tag: "latest",
          resolvedTag: "v5.2.3",
          digest:
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
          archMatch: "match",
          arch: ["linux/amd64"],
        },
      },
    },
  },
  render: renderServices("回归：same-tag / digest-only 候选必须明确可见"),
  play: async ({ canvasElement }) => {
    await sleep(260);
    assertNoStackPolicyButtons(canvasElement);

    expectStory(
      canvasElement.textContent?.includes("同标签新 digest"),
      "services table should surface same-tag digest-only updates explicitly",
    );

    const stackAction = findStackUpdateButton(canvasElement);
    expectStory(stackAction, "expected stack action button for same-tag case");
    stackAction.click();
    await sleep(220);

    const dialog =
      document.querySelector<HTMLElement>('[role="alertdialog"]') ??
      document.querySelector<HTMLElement>('[role="dialog"]');
    expectStory(dialog, "expected same-tag confirm dialog");
    expectStory(
      dialog.textContent?.includes("同标签新 digest"),
      "aggregate preview should also expose same-tag digest-only updates",
    );

    const cancelButton = Array.from(
      dialog.querySelectorAll<HTMLButtonElement>("button"),
    ).find((button) => button.textContent?.includes("取消"));
    cancelButton?.click();
    await sleep(120);
  },
};

export const GlobalTaskReadableLabel: Story = {
  parameters: {
    dockrevApiScenario: "overview-jobs-card-global-labels",
    viewport: { defaultViewport: "dockrevWide" },
  },
  render: renderServices("回归：全局任务使用可读标签，不显示内部类型名"),
  play: async ({ canvasElement }) => {
    await sleep(260);

    const globalJob = Array.from(
      canvasElement.querySelectorAll<HTMLElement>(".overviewJobListRow"),
    ).find((row) => row.textContent?.includes("全部"));
    expectStory(globalJob, "expected a global discovery task in the operations dashboard");
    const label = globalJob?.querySelector<HTMLElement>(".overviewJobTitle")?.textContent ?? "";
    expectStory(label.includes("发现扫描"), "expected the global discovery task to use its readable label");
    expectStory(!label.includes("discovery"), "expected the internal discovery type to stay out of the visible label");
  },
};
