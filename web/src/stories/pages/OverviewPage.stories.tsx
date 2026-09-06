import type { Meta, StoryObj } from "@storybook/react";
import { OverviewPage } from "../../pages/OverviewPage";
import { PageHarness } from "../mocks/PageHarness";
import { withDockrevMockApi } from "../mocks/withDockrevMockApi";
import {
  auditProofHomepageOverrides,
  cachedHomepageSnapshot,
  defaultHomepageOverrides,
  denseHomepageOverrides,
} from "./OverviewPage.storyData";
export { IconKinds, UnsafeHomepageHrefFallsBack } from "./overviewIconStories";

const meta: Meta<typeof OverviewPage> = {
  title: "Pages/OverviewPage",
  component: OverviewPage,
  decorators: [withDockrevMockApi],
  parameters: { layout: "fullscreen" },
  tags: ["autodocs"],
};

export default meta;

type Story = StoryObj<typeof OverviewPage>;

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

function dispatchPointer(target: EventTarget, type: string, init: PointerEventInit) {
  target.dispatchEvent(
    new PointerEvent(type, {
      bubbles: true,
      cancelable: true,
      composed: true,
      pointerType: "mouse",
      ...init,
    }),
  );
}

function renderOverview(options?: {
  runtimeMode?: "app-demo";
}): Story["render"] {
  return () => (
    <PageHarness
      route={{ name: "overview" }}
      title=""
      runtimeMode={options?.runtimeMode ?? null}
    >
      {({
        onLastScanHint,
        onContextNavigation,
        onMobileNavContent,
        onSidebarNavContent,
        onTopActions,
        onTopbarContent,
      }) => (
        <OverviewPage
          onLastScanHint={onLastScanHint}
          onContextNavigation={onContextNavigation}
          onMobileNavContent={onMobileNavContent}
          onSidebarNavContent={onSidebarNavContent}
          onTopActions={onTopActions}
          onTopbarContent={onTopbarContent}
        />
      )}
    </PageHarness>
  );
}

function serviceCards(canvasElement: HTMLElement) {
  return Array.from(
    canvasElement.querySelectorAll<HTMLElement>(".homepageServiceCard"),
  );
}

function desktopTopMetricValue(canvasElement: HTMLElement, label: string) {
  const metrics = Array.from(
    canvasElement.querySelectorAll<HTMLElement>(
      ".topbar .homepageTopMetric",
    ),
  );
  return (
    metrics
      .find(
        (metric) =>
          metric.querySelector(".homepageTopMetricLabel")?.textContent ===
          label,
      )
      ?.querySelector(".homepageTopMetricValue")?.textContent ?? null
  );
}

function findCardByText(canvasElement: HTMLElement, text: string) {
  return serviceCards(canvasElement).find((card) =>
    card.textContent?.includes(text),
  );
}

export const Default: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceOverridesById: defaultHomepageOverrides(),
  },
  render: renderOverview(),
  play: async ({ canvasElement }) => {
    await sleep(260);

    expectStory(
      !canvasElement.querySelector(".pageHead"),
      "expected app overview story to omit the old page title block",
    );
    expectStory(
      canvasElement.querySelector('h1.srOnly')?.textContent === "服务导航",
      "expected overview page to expose a hidden page heading",
    );
    const doc = canvasElement.ownerDocument;
    const isNarrow = doc.defaultView?.matchMedia("(max-width: 960px)").matches;
    expectStory(
      !doc.querySelector(".sidebarNavHeader, .sidebarNavLabel"),
      "expected the sidebar to remove the redundant navigation heading",
    );
    expectStory(
      !doc.querySelector(".sidebar .homepageSidebarClock"),
      "expected the sidebar to keep browser time out of page context",
    );
    expectStory(
      !canvasElement.querySelector(".homepageMobileNavModule"),
      "expected overview resource controls to stay out of the page body",
    );
    if (isNarrow) {
      expectStory(
        !doc.querySelector(".topbar .homepageHeaderContent"),
        "expected narrow overview to keep page tools out of the header",
      );
      const menuButton = doc.querySelector<HTMLButtonElement>(".mobileMenuButton");
      expectStory(menuButton, "expected narrow overview to expose the context drawer trigger");
      menuButton?.click();
      await sleep(80);
      expectStory(
        doc.querySelector(".mobileMenuEmbeddedContent .homepageDrawerSearchSlot"),
        "expected narrow overview search to belong to the context drawer",
      );
      expectStory(
        doc.querySelector(".mobileMenuEmbeddedContent .homepageDrawerBottomSummary"),
        "expected narrow overview resource summary to belong to the context drawer",
      );
      expectStory(
        doc.querySelector(".mobileMenuEmbeddedContent .homepageClock[aria-label='抽屉浏览器本地当前时间']"),
        "expected narrow overview clock to belong to the context drawer",
      );
      expectStory(
        doc.querySelectorAll('input[type="search"][aria-label="搜索服务入口"]').length === 1,
        "expected narrow overview to mount exactly one search input",
      );
    } else {
      const header = doc.querySelector<HTMLElement>(".topbar .homepageHeaderContent");
      expectStory(header, "expected desktop overview resource summary in the header");
      expectStory(
        doc.querySelectorAll(".topbar .homepageTopMetric").length === 4,
        "expected desktop header to expose four resource metrics",
      );
      if (header?.dataset.layout === "compact") {
        expectStory(
          !doc.querySelector(".topbar .homepageHeaderClock"),
          "expected constrained desktop header to hide the clock",
        );
        expectStory(
          doc.querySelector(".topbar .homepageHeaderSearchToggle"),
          "expected constrained desktop header to expose a search trigger",
        );
      } else {
        expectStory(
          doc.querySelector(".topbar .homepageHeaderClock[aria-label='浏览器本地当前时间']"),
          "expected wide desktop header to show browser-local time",
        );
        expectStory(
          doc.querySelector(".topbar .homepageClockZone")?.textContent?.startsWith("GMT"),
          "expected browser-local time to show its GMT offset",
        );
        expectStory(
          doc.querySelectorAll('input[type="search"][aria-label="搜索服务入口"]').length === 1,
          "expected wide desktop overview to mount exactly one search input",
        );
      }
    }
    expectStory(
      canvasElement.querySelector('button[aria-label="刷新服务列表"]'),
      "expected refresh top action to keep an accessible name when labels collapse",
    );
    expectStory(
      canvasElement.querySelector('button[aria-label="立即扫描更新"]'),
      "expected scan top action to keep an accessible name when labels collapse",
    );
    expectStory(
      !canvasElement.querySelector(".homepageToolFloatWindow") &&
        !canvasElement.querySelector(".homepageToolBubble"),
      "expected the production overview shell to keep the demo-only floating tools UI out of the page",
    );
    expectStory(
      !canvasElement.querySelector(".homepageOverviewSearchButton"),
      "expected overview search to rely on Enter instead of a separate search button",
    );
    expectStory(
      canvasElement.querySelector(".topActions")?.textContent?.includes("运维大盘") !== true,
      "expected overview top actions to omit the redundant operations dashboard shortcut",
    );

    const groups = Array.from(
      canvasElement.querySelectorAll<HTMLElement>(".homepageDashboardGroup"),
    );
    expectStory(groups.length >= 2, "expected grouped Homepage columns");
    expectStory(
      canvasElement.querySelectorAll(".homepageDashboardColumn").length >= 2,
      "expected grouped Homepage sections to render through balanced columns",
    );
    expectStory(
      groups.some((group) => group.textContent?.includes("Brain")),
      "expected homepage.group to drive column headings",
    );
    expectStory(
      canvasElement.textContent?.includes("prod ·") !== true,
      "expected service cards to keep stack/service internals out of the visible chrome",
    );

    const cards = serviceCards(canvasElement);
    expectStory(cards.length >= 4, "expected homepage service cards to render");
    for (const card of cards) {
      expectStory(
        card.querySelectorAll(".homepageServiceMetric").length === 4,
        "expected every card to show CPU/MEM/RX/TX cells",
      );
      expectStory(
        card.querySelector(".homepageServiceStateBadge"),
        "expected every card to reuse update status badge surface",
      );
      expectStory(
        card.querySelector(".homepageServiceDetailButton"),
        "expected every card to expose a compact service detail action",
      );
    }
    expectStory(
      cards.every((card) => card.getAttribute("role") === "link"),
      "expected homepage service cards to keep direct launcher semantics",
    );
    expectStory(
      canvasElement.querySelector(".homepageServiceStateButton"),
      "expected updatable cards to expose the status badge as a clickable update action",
    );
    canvasElement
      .querySelector<HTMLButtonElement>(".homepageServiceStateButton")
      ?.click();
    await sleep(120);
    expectStory(
      document.body.textContent?.includes("确认更新服务"),
      "clicking the updatable badge should open the update confirmation dialog",
    );
    expectStory(
      document.body.textContent?.includes("版本"),
      "homepage update confirmation should show the version summary",
    );
    expectStory(
      document.body.textContent?.includes("目标 digest"),
      "homepage update confirmation should show the target digest",
    );
  },
};

export const PublicDemoControlPanel: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceOverridesById: defaultHomepageOverrides(),
    docs: {
      description: {
        story:
          "Use the demo runtime marker to verify the floating demo-only control panel that belongs only to the public /demo/ surface.",
      },
    },
  },
  render: renderOverview({ runtimeMode: "app-demo" }),
  play: async ({ canvasElement }) => {
    await sleep(260);

    expectStory(
      !canvasElement.ownerDocument.querySelector(".sidebar .homepageSidebarClock"),
      "expected the demo overview shell to move time out of the left sidebar",
    );
    expectStory(
      canvasElement.querySelector(".homepageToolFloatWindow .homepageToolFloatTitle")
        ?.textContent === "Demo 控制面板",
      "expected the public demo control panel to expose the visible title",
    );
    expectStory(
      canvasElement.querySelector(".homepageToolFloatWindow .homepageToolFloatEyebrow")
        ?.textContent === "Public Demo",
      "expected the public demo control panel to advertise the demo runtime explicitly",
    );
    expectStory(
      !canvasElement.querySelector(".homepageToolFloatMetrics") &&
        !canvasElement.textContent?.includes("Demo Runtime"),
      "expected the public demo control panel to remove the low-value runtime status block",
    );
    expectStory(
      !canvasElement.querySelector(".homepageToolFloatSearchSlot") &&
        !canvasElement.querySelector(".homepageSidebarClock"),
      "expected the public demo control panel to remove unrelated search and clock content",
    );
    expectStory(
      !canvasElement.querySelector(".homepageToolFloatDock"),
      "expected the public demo control panel to keep the old dock badge removed",
    );
    expectStory(
      !canvasElement.querySelector(".homepageToolBubble"),
      "expected the public demo control panel to start expanded instead of hidden as a bubble",
    );
    expectStory(
      canvasElement.textContent?.includes("GHCR 假写") &&
        canvasElement.textContent?.includes("Cleanup 假写") &&
        canvasElement.textContent?.includes("重置 Seed"),
      "expected the public demo control panel to expose demo route and session controls",
    );
    expectStory(
      canvasElement.textContent?.includes("不会影响真实环境") &&
        !canvasElement.textContent?.includes("sessionStorage"),
      "expected the public demo control panel to use concise descriptive copy instead of implementation-detail requirements",
    );

    const floatWindow = canvasElement.querySelector<HTMLElement>(".homepageToolFloatWindow");
    const floatHead = canvasElement.querySelector<HTMLElement>(".homepageToolFloatHead");
    const contentHost = canvasElement.ownerDocument.querySelector<HTMLElement>(".content.overlayScrollArea");
    const viewportHost = canvasElement.ownerDocument.documentElement;
    expectStory(floatWindow, "expected overview floating tools panel to render");
    expectStory(floatHead, "expected overview floating tools panel to expose a drag handle");
    expectStory(contentHost, "expected overview story to mount inside the shell content viewport");
    expectStory(viewportHost, "expected overview story to expose a viewport root for edge snapping");
    const before = floatWindow?.getBoundingClientRect();
    const hostRect = viewportHost?.getBoundingClientRect();
    if (before && hostRect && floatHead) {
      dispatchPointer(floatHead, "pointerdown", {
        pointerId: 11,
        button: 0,
        buttons: 1,
        clientX: before.left + 28,
        clientY: before.top + 20,
      });
      dispatchPointer(window, "pointermove", {
        pointerId: 11,
        button: 0,
        buttons: 1,
        clientX: hostRect.left + 18,
        clientY: before.top + 36,
      });
      dispatchPointer(window, "pointerup", {
        pointerId: 11,
        button: 0,
        buttons: 0,
        clientX: hostRect.left + 18,
        clientY: before.top + 36,
      });
      await sleep(220);
      expectStory(
        canvasElement.querySelector(".homepageToolFloatWindow"),
        "expected the public demo control panel to stay expanded after dragging to the page edge",
      );
      expectStory(
        !canvasElement.querySelector(".homepageToolBubble"),
        "expected the public demo control panel to avoid auto-collapsing into a bubble when dragged to the edge",
      );
    }
    const collapseButton = canvasElement.querySelector<HTMLButtonElement>('button[aria-label="收起 Demo 控制面板"]');
    const collapseButtonBox = collapseButton?.getBoundingClientRect() ?? null;
    collapseButton?.click();
    await sleep(180);
    expectStory(
      !canvasElement.querySelector(".homepageToolFloatWindow"),
      "expected the public demo control panel to collapse into a bubble",
    );
    const bubble = canvasElement.querySelector<HTMLElement>(".homepageToolBubble");
    const bubbleBox = bubble?.getBoundingClientRect() ?? null;
    if (collapseButtonBox && bubbleBox) {
      const bubbleCenterY = bubbleBox.top + bubbleBox.height / 2;
      const buttonCenterY = collapseButtonBox.top + collapseButtonBox.height / 2;
      expectStory(
        Math.abs(bubbleCenterY - buttonCenterY) <= 8,
        "expected the collapsed bubble to align vertically to the collapse button position",
      );
    }
    const draggableBubble = canvasElement.querySelector<HTMLElement>(".homepageToolBubbleButton");
    if (bubbleBox && hostRect && draggableBubble) {
      dispatchPointer(draggableBubble, "pointerdown", {
        pointerId: 12,
        button: 0,
        buttons: 1,
        clientX: bubbleBox.left + 14,
        clientY: bubbleBox.top + bubbleBox.height / 2,
      });
      dispatchPointer(window, "pointermove", {
        pointerId: 12,
        button: 0,
        buttons: 1,
        clientX: hostRect.right - 18,
        clientY: bubbleBox.top + bubbleBox.height / 2,
      });
      dispatchPointer(window, "pointerup", {
        pointerId: 12,
        button: 0,
        buttons: 0,
        clientX: hostRect.right - 18,
        clientY: bubbleBox.top + bubbleBox.height / 2,
      });
      await sleep(220);
      expectStory(
        canvasElement.querySelector(".homepageToolBubble")?.getAttribute("data-side") === "right",
        "expected the collapsed demo bubble to support dragging and auto-snap back onto the nearest right edge",
      );
    }
    const bubbleButton = canvasElement.querySelector<HTMLButtonElement>(".homepageToolBubbleButton");
    expectStory(bubbleButton, "expected the public demo control panel to expose a collapsed bubble trigger");
    expectStory(
      canvasElement.querySelector(".homepageToolBubbleCount")?.textContent === "DEMO",
      "expected the collapsed bubble to keep a demo marker instead of production counts",
    );
    bubbleButton?.click();
    await sleep(180);
    expectStory(
      canvasElement.querySelector(".homepageToolFloatWindow") && !canvasElement.querySelector(".homepageToolBubble"),
      "expected the public demo bubble to expand back into the full control window",
    );
  },
};

export const CachedInstantNavigation: Story = {
  parameters: {
    dockrevApiScenario: "overview-homepage-slow-refresh",
    dockrevHomepageSnapshot: cachedHomepageSnapshot(),
    dockrevServiceOverridesById: defaultHomepageOverrides(),
  },
  render: renderOverview(),
  play: async ({ canvasElement }) => {
    await sleep(80);

    expectStory(
      findCardByText(canvasElement, "Cached Acme API"),
      "cached launcher card should be visible before the slow homepage payload returns",
    );
    expectStory(
      desktopTopMetricValue(canvasElement, "CPU") === "42%",
      "cached resource summary should populate top CPU before the metrics request returns",
    );
    expectStory(
      canvasElement.textContent?.includes("正在刷新服务入口，先显示上次导航"),
      "cached navigation should disclose that a background refresh is running",
    );
    expectStory(
      !canvasElement.textContent?.includes("当前搜索条件下没有可展示的服务入口"),
      "cached first paint must not show the misleading empty state",
    );

    await sleep(1000);
    expectStory(
      findCardByText(canvasElement, "Acme API"),
      "live cards should replace cached launcher cards after refresh",
    );
  },
};

export const ColdStartSkeleton: Story = {
  parameters: {
    dockrevApiScenario: "overview-homepage-slow-refresh",
    dockrevServiceOverridesById: defaultHomepageOverrides(),
  },
  render: renderOverview(),
  play: async ({ canvasElement }) => {
    await sleep(80);

    expectStory(
      canvasElement.querySelector(".homepageNavSkeleton"),
      "cold start should render a stable navigation skeleton while data loads",
    );
    expectStory(
      !canvasElement.textContent?.includes("当前搜索条件下没有可展示的服务入口"),
      "cold start skeleton must not show an empty result before refresh completes",
    );

    await sleep(1000);
    expectStory(
      serviceCards(canvasElement).length >= 4,
      "live homepage cards should render after the slow cold-start refresh",
    );
  },
};

export const AuditProof: Story = {
  globals: {
    backgrounds: { value: "light" },
    theme: "light",
  },
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceOverridesById: auditProofHomepageOverrides(),
    docs: {
      description: {
        story:
          "Single proof story for the Homepage audit repair: balanced columns, accessibility names, light contrast surface, proxied icons, direct icons, and fallback icons.",
      },
    },
  },
  render: renderOverview(),
  play: async ({ canvasElement }) => {
    await sleep(260);

    expectStory(
      document.documentElement.dataset.theme === "light",
      "expected audit proof to render with light theme tokens",
    );
    expectStory(
      canvasElement.querySelector('h1.srOnly')?.textContent === "服务导航",
      "expected audit proof to expose the hidden page heading",
    );
    expectStory(
      canvasElement.querySelector<HTMLInputElement>(
        'input[type="search"][aria-label="搜索服务入口"]',
      ),
      "expected audit proof to expose the search input accessible name",
    );
    expectStory(
      canvasElement.querySelector('button[aria-label="刷新服务列表"]'),
      "expected audit proof to expose the refresh accessible name",
    );
    expectStory(
      canvasElement.querySelector('button[aria-label="立即扫描更新"]'),
      "expected audit proof to expose the scan accessible name",
    );

    const columns = Array.from(
      canvasElement.querySelectorAll<HTMLElement>(".homepageDashboardColumn"),
    );
    expectStory(
      columns.length >= 3,
      "expected audit proof to render multiple balanced desktop columns",
    );
    expectStory(
      columns.some(
        (column) =>
          column.querySelectorAll(".homepageDashboardGroup").length > 1,
      ),
      "expected audit proof to show at least one column with stacked groups",
    );
    expectStory(
      Array.from(
        canvasElement.querySelectorAll(".homepageDashboardGroupHeader"),
      ).some(
        (group) =>
          group.textContent?.includes("Brain") &&
          group.textContent?.includes("2"),
      ),
      "expected audit proof to include a taller Brain group beside short groups",
    );
    expectStory(
      canvasElement.querySelector(".homepageStatusLine"),
      "expected audit proof to expose the light-theme status line",
    );
    expectStory(
      canvasElement.querySelector(".homepageDashboardGroupHeader span"),
      "expected audit proof to expose group count badges",
    );

    const acmeIcon = findCardByText(canvasElement, "Acme API")?.querySelector(
      ".homepageServiceIcon",
    );
    expectStory(
      acmeIcon?.getAttribute("data-icon-src")?.includes(
        "/api/homepage-icons/iconify/simple-icons/github.svg?color=%23dbeafe",
      ),
      "expected audit proof to route simple-icons through the local proxy",
    );

    const webIcon = findCardByText(canvasElement, "Web Console")?.querySelector(
      ".homepageServiceIcon",
    );
    expectStory(
      webIcon?.getAttribute("data-icon-src")?.includes(
        "/api/homepage-icons/iconify/mdi/monitor-dashboard.svg?color=%23dbeafe",
      ),
      "expected audit proof to route mdi icons through the local proxy",
    );

    const workerIcon = findCardByText(
      canvasElement,
      "Background Jobs",
    )?.querySelector(".homepageServiceIcon");
    expectStory(
      workerIcon?.getAttribute("data-icon-src")?.includes(
        "/api/homepage-icons/selfhst/png/home-assistant.png",
      ),
      "expected audit proof to route selfh.st icons through the local proxy",
    );

    const promIcon = findCardByText(canvasElement, "Prometheus")?.querySelector(
      ".homepageServiceIcon",
    );
    expectStory(
      promIcon?.getAttribute("data-icon-src")?.includes(
        "/api/homepage-icons/dashboard/svg/prometheus.svg",
      ),
      "expected audit proof to route dashboard-icons through the local proxy",
    );

    const lokiIcon = findCardByText(canvasElement, "Loki")?.querySelector(
      ".homepageServiceIcon",
    );
    expectStory(
      lokiIcon?.getAttribute("data-icon-kind") === "fallback" &&
        !lokiIcon.querySelector("img"),
      "expected audit proof to show fallback icon behavior for unsafe icon specs",
    );

    const postgresIcon = findCardByText(
      canvasElement,
      "Postgres",
    )?.querySelector(".homepageServiceIcon");
    expectStory(
      postgresIcon?.getAttribute("data-icon-kind") === "url" &&
        postgresIcon
          .getAttribute("data-icon-src")
          ?.startsWith("https://cdn.jsdelivr.net/") === true,
      "expected audit proof to keep absolute icon URLs as direct browser loads",
    );
  },
};

export const DenseBalancedGroups: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceOverridesById: denseHomepageOverrides(),
    docs: {
      description: {
        story:
          "Review this story at desktop width to verify short groups continue below taller groups without grid-row whitespace.",
      },
    },
  },
  render: renderOverview(),
  play: async ({ canvasElement }) => {
    await sleep(260);

    const columns = Array.from(
      canvasElement.querySelectorAll<HTMLElement>(".homepageDashboardColumn"),
    );
    expectStory(columns.length >= 3, "expected desktop overview to use multiple balanced columns");
    expectStory(
      columns.some(
        (column) =>
          column.querySelectorAll(".homepageDashboardGroup").length > 1,
      ),
      "expected at least one balanced column to contain stacked groups",
    );
    expectStory(
      Array.from(canvasElement.querySelectorAll(".homepageDashboardGroupHeader")).some(
        (group) =>
          group.textContent?.includes("Brain") &&
          group.textContent?.includes("2"),
      ),
      "expected the dense story to keep a visibly taller group for density review",
    );
  },
};

export const SearchAndFallback: Story = {
  parameters: { dockrevApiScenario: "multi-stack-mixed" },
  render: renderOverview(),
  play: async ({ canvasElement }) => {
    await sleep(260);

    const allCards = serviceCards(canvasElement);
    expectStory(
      allCards.length >= 4,
      "expected homepage overview cards to render",
    );

    const workerCard = allCards.find((card) =>
      card.textContent?.includes("worker"),
    );
    expectStory(
      !workerCard,
      "services without a valid homepage.href should stay out of the launcher",
    );

    const searchInput = canvasElement.querySelector<HTMLInputElement>(
      'input[type="search"][aria-label="搜索服务入口"]',
    );
    expectStory(searchInput, "expected overview search input");

    setInputValue(searchInput, "worker");
    searchInput.form?.requestSubmit();
    await sleep(260);

    expectStory(
      serviceCards(canvasElement).length === 0,
      "keyboard search should exclude services without launch hrefs",
    );

    setInputValue(searchInput, "Acme API");
    searchInput.form?.requestSubmit();
    await sleep(260);

    const filteredCards = serviceCards(canvasElement);
    expectStory(
      filteredCards.length === 1,
      "search should filter overview cards down to one Web entry",
    );
    expectStory(
      filteredCards[0].textContent?.includes("Acme API"),
      "filtered overview card should be Acme API",
    );

    setInputValue(searchInput, "ghcr.io/acme/api");
    searchInput.form?.requestSubmit();
    await sleep(260);

    const imageFilteredCards = serviceCards(canvasElement);
    expectStory(
      imageFilteredCards.length === 1,
      "search should still match homepage cards by image ref",
    );
    expectStory(
      imageFilteredCards[0].textContent?.includes("Acme API"),
      "image-ref search should keep the Acme API card visible",
    );
  },
};

export const DockrevSelfUpdateGuard: Story = {
  parameters: {
    dockrevApiScenario: "aggregate-dockrev-guard",
    dockrevServiceOverridesById: {
      "svc-aggregate-guard-api": {
        homepage: {
          group: "Core",
          name: "Acme API",
          icon: "si-github",
          href: "https://api.example.com",
          description: "Regular app update target",
        },
      },
      "svc-aggregate-guard-dockrev": {
        homepage: {
          group: "Core",
          name: "Dockrev",
          icon: "mdi-docker",
          href: "https://dockrev.example.com",
          description: "Dockrev control plane",
        },
      },
    },
  },
  render: renderOverview(),
  play: async ({ canvasElement }) => {
    await sleep(260);

    const dockrevCard = serviceCards(canvasElement).find((card) =>
      card.textContent?.includes("Dockrev"),
    );
    expectStory(dockrevCard, "expected Dockrev launcher card to remain visible");
    expectStory(
      dockrevCard.querySelector(".homepageServiceStateBadge")?.textContent?.includes("可更新"),
      "expected Dockrev card to keep the updatable status visible",
    );
    expectStory(
      !dockrevCard.querySelector(".homepageServiceStateButton"),
      "Dockrev card must not expose the ordinary service update action",
    );

    const apiCard = serviceCards(canvasElement).find((card) =>
      card.textContent?.includes("Acme API"),
    );
    expectStory(apiCard, "expected regular app launcher card");
    expectStory(
      apiCard.querySelector(".homepageServiceStateButton"),
      "regular updatable services should keep the homepage update action",
    );
  },
};

export const MetricsDisabled: Story = {
  parameters: { dockrevApiScenario: "service-detail-resource-monitor-disabled" },
  render: renderOverview(),
  play: async ({ canvasElement }) => {
    await sleep(260);

    expectStory(
      canvasElement.textContent?.includes("资源监控已关闭"),
      "expected disabled monitor overview status",
    );
    const firstCard = serviceCards(canvasElement)[0];
    expectStory(firstCard, "expected cards to remain visible when monitor is disabled");
    expectStory(
      firstCard
        .querySelector(".homepageServiceStateBadge")
        ?.textContent?.includes("NO DATA") === true,
      "expected disabled monitor card badge to degrade without breaking",
    );
    expectStory(
      Array.from(firstCard.querySelectorAll(".homepageServiceMetricValue")).every(
        (node) => node.textContent === "-",
      ),
      "expected disabled monitor card metrics to render placeholders",
    );
  },
};

export const MetricsUnavailable: Story = {
  parameters: {
    dockrevApiScenario: "overview-resource-monitor-error",
    dockrevServiceOverridesById: defaultHomepageOverrides(),
    dockrevHomepageSnapshot: cachedHomepageSnapshot(),
  },
  render: renderOverview(),
  play: async ({ canvasElement }) => {
    await sleep(80);

    expectStory(
      findCardByText(canvasElement, "Cached Acme API"),
      "homepage should keep cached cards visible when the single nav payload fails",
    );
    expectStory(
      desktopTopMetricValue(canvasElement, "CPU") === "42%",
      "homepage should keep cached resource summary visible when the single nav payload fails",
    );
    expectStory(
      canvasElement.textContent?.includes("首页导航刷新失败："),
      "expected homepage refresh failure to stay visible while cached data remains available",
    );
    const badges = Array.from(
      canvasElement.querySelectorAll<HTMLElement>(".homepageServiceStateBadge"),
    );
    expectStory(
      badges.some((badge) => badge.textContent?.includes("NO DATA")),
      "expected cached metric state to remain stable instead of collapsing the card grid",
    );
  },
};

export const LightContrast: Story = {
  globals: {
    backgrounds: { value: "light" },
    theme: "light",
  },
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceOverridesById: defaultHomepageOverrides(),
    docs: {
      description: {
        story:
          "Light-theme evidence for primary actions, status text, and group-count badge contrast.",
      },
    },
  },
  render: renderOverview(),
  play: async ({ canvasElement }) => {
    await sleep(260);

    expectStory(
      document.documentElement.dataset.theme === "light",
      "expected light contrast story to render with light theme tokens",
    );
    expectStory(
      canvasElement.querySelector(".homepageStatusLine"),
      "expected light contrast story to include the homepage status line",
    );
    expectStory(
      canvasElement.querySelector(".homepageDashboardGroupHeader span"),
      "expected light contrast story to expose group count badges",
    );
    expectStory(
      canvasElement.querySelector('button[aria-label="立即扫描更新"]'),
      "expected light contrast story to expose the primary scan action",
    );
  },
};

export const MetricsStale: Story = {
  parameters: { dockrevApiScenario: "overview-resource-monitor-stale" },
  render: renderOverview(),
  play: async ({ canvasElement }) => {
    await sleep(260);

    const staleCard = serviceCards(canvasElement).find((card) =>
      card
        .querySelector(".homepageServiceStateBadge")
        ?.textContent?.includes("NO DATA"),
    );
    expectStory(staleCard, "expected stale resource samples to be suppressed from the card");
    expectStory(
      staleCard.querySelectorAll(".homepageServiceMetricValue").length === 4,
      "expected cards with unavailable samples to keep metric cells stable",
    );
  },
};

export const MetricAggregationTotals: Story = {
  parameters: {
    dockrevApiScenario: "overview-resource-monitor-zero-rates",
    dockrevServiceOverridesById: defaultHomepageOverrides(),
  },
  render: renderOverview(),
  play: async ({ canvasElement }) => {
    await sleep(260);

    const cards = serviceCards(canvasElement);
    expectStory(cards.length > 1, "expected multiple active services");
    expectStory(
      desktopTopMetricValue(canvasElement, "CPU") === `${cards.length * 25}%`,
      "expected top CPU summary to sum service usage instead of averaging it",
    );
    expectStory(
      desktopTopMetricValue(canvasElement, "MEM") === "0 B",
      "expected top memory summary to preserve a valid zero value",
    );
    expectStory(
      desktopTopMetricValue(canvasElement, "RX") === "0 B/s",
      "expected top RX summary to preserve a valid zero rate",
    );
    expectStory(
      desktopTopMetricValue(canvasElement, "TX") === "0 B/s",
      "expected top TX summary to preserve a valid zero rate",
    );
  },
};

export const WideHeader: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceOverridesById: defaultHomepageOverrides(),
  },
  render: renderOverview(),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    if (doc.defaultView?.matchMedia("(max-width: 960px)").matches) return;
    await sleep(260);

    const headerSlot = doc.querySelector<HTMLElement>(".topbarGlobalContent");
    headerSlot?.style.setProperty("flex", "1 1 760px");
    headerSlot?.style.setProperty("min-width", "760px");
    await sleep(80);

    const header = doc.querySelector<HTMLElement>(".homepageHeaderContent");
    expectStory(header?.dataset.layout === "full", "expected wide desktop header layout");
    expectStory(
      header?.querySelectorAll(".homepageTopMetric").length === 4,
      "expected wide desktop header to retain the resource summary",
    );
    const clock = header?.querySelector<HTMLElement>(
      ".homepageHeaderClock[aria-label='浏览器本地当前时间']",
    );
    expectStory(clock, "expected wide desktop header to show browser-local time");
    expectStory(
      clock?.querySelector(".homepageClockZone")?.textContent?.startsWith("GMT"),
      "expected browser-local time to display its GMT offset",
    );
    expectStory(
      header?.querySelectorAll('input[type="search"][aria-label="搜索服务入口"]').length === 1,
      "expected wide desktop header to mount exactly one service search input",
    );
  },
};

export const ConstrainedHeader: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceOverridesById: defaultHomepageOverrides(),
  },
  render: renderOverview(),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument;
    if (doc.defaultView?.matchMedia("(max-width: 960px)").matches) return;
    await sleep(260);

    const headerSlot = doc.querySelector<HTMLElement>(".topbarGlobalContent");
    headerSlot?.style.setProperty("flex", "0 0 620px");
    headerSlot?.style.setProperty("width", "620px");
    await sleep(80);

    const header = doc.querySelector<HTMLElement>(".homepageHeaderContent");
    expectStory(header?.dataset.layout === "compact", "expected constrained desktop header layout");
    expectStory(
      header?.querySelectorAll(".homepageTopMetric").length === 4,
      "expected constrained desktop header to retain the resource summary",
    );
    expectStory(
      !header?.querySelector(".homepageHeaderClock"),
      "expected constrained desktop header to hide browser-local time",
    );
    const trigger = header?.querySelector<HTMLButtonElement>(
      ".homepageHeaderSearchToggle[aria-label='打开服务搜索']",
    );
    expectStory(trigger, "expected constrained desktop header to expose a search trigger");
    expectStory(
      doc.querySelectorAll('input[type="search"][aria-label="搜索服务入口"]').length === 0,
      "expected a closed constrained search popover to mount no hidden input",
    );
    const cardsBefore = serviceCards(canvasElement).length;
    trigger?.click();
    await sleep(80);
    expectStory(
      doc.querySelectorAll('input[type="search"][aria-label="搜索服务入口"]').length === 1,
      "expected an open constrained search popover to mount exactly one input",
    );
    expectStory(
      serviceCards(canvasElement).length === cardsBefore,
      "expected opening the search popover not to change the service filter",
    );
  },
};

export const MobileStacked: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceOverridesById: defaultHomepageOverrides(),
    docs: {
      description: {
        story:
          "Capture this story with a narrow viewport to verify the single-column mobile layout.",
      },
    },
  },
  render: renderOverview(),
  play: async ({ canvasElement }) => {
    await sleep(260);
    const doc = canvasElement.ownerDocument;
    if (!doc.defaultView?.matchMedia("(max-width: 960px)").matches) return;

    expectStory(
      canvasElement.querySelector(".homepageDashboardGrid"),
      "expected mobile evidence story to expose the dashboard grid",
    );
    expectStory(
      !canvasElement.querySelector(".homepageMobileNavModule"),
      "expected mobile overview to keep resource controls out of the page body",
    );
    expectStory(
      !doc.querySelector(".topbar .homepageHeaderContent"),
      "expected mobile overview to keep search and time out of the header",
    );
    expectStory(
      !doc.querySelector(".sidebar .homepageSidebarClock"),
      "expected mobile overview to keep time out of the sidebar",
    );
    expectStory(
      !canvasElement.querySelector(".homepageToolFloatWindow"),
      "expected mobile evidence story to hide the desktop floating tools panel",
    );
    expectStory(
      !canvasElement.querySelector(".homepageToolBubble"),
      "expected mobile evidence story to keep the collapsed desktop bubble out of the narrow viewport",
    );
    const menuButton = doc.querySelector<HTMLButtonElement>(".mobileMenuButton");
    expectStory(menuButton, "expected mobile overview to expose the context drawer trigger");
    menuButton?.click();
    await sleep(80);
    expectStory(
      doc.querySelector(".mobileMenuEmbeddedContent .homepageDrawerSearchSlot"),
      "expected mobile page context navigation to own the search control",
    );
    expectStory(
      doc.querySelectorAll('input[type="search"][aria-label="搜索服务入口"]').length === 1,
      "expected mobile overview to mount exactly one search input",
    );
    expectStory(
      doc.querySelector(".mobileMenuEmbeddedContent .homepageDrawerBottomSummary"),
      "expected hamburger menu to own the mobile resource summary",
    );
    expectStory(
      doc.querySelector(".mobileMenuEmbeddedContent .homepageClock[aria-label='抽屉浏览器本地当前时间']"),
      "expected mobile context drawer to own browser-local time",
    );
    expectStory(
      serviceCards(canvasElement).length >= 4,
      "expected mobile evidence story to render service cards",
    );
  },
};
