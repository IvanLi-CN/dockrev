import type { Meta, StoryObj } from "@storybook/react";
import { OverviewPage } from "../../pages/OverviewPage";
import type {
  HomepageNavSnapshot,
  HomepageResourceSummarySnapshot,
} from "../../pages/homepageSnapshot";
import { PageHarness } from "../mocks/PageHarness";
import { withDockrevMockApi } from "../mocks/withDockrevMockApi";

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

function renderOverview(): Story["render"] {
  return () => (
    <PageHarness route={{ name: "overview" }} title="" topbarHint="服务导航">
      {({
        onLastScanHint,
        onMobileNavContent,
        onSidebarNavContent,
        onTopActions,
        onTopbarContent,
      }) => (
        <OverviewPage
          onLastScanHint={onLastScanHint}
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

function defaultHomepageOverrides() {
  return {
    "svc-prod-api": {
      homepage: {
        group: "Brain",
        name: "Acme API",
        icon: "si-github",
        href: "https://api.example.com",
        description: "API gateway & auth",
      },
    },
    "svc-prod-web": {
      homepage: {
        group: "Brain",
        name: "Web Console",
        icon: "mdi-monitor-dashboard",
        href: "https://web.example.com",
        description: "Primary admin console",
      },
    },
    "svc-prod-worker": {
      homepage: {
        group: "Tools",
        name: "Background Jobs",
        icon: "mdi-cog-refresh-outline",
        href: null,
        description: "Queue workers & cron",
      },
    },
    "svc-infra-loki": {
      homepage: {
        group: "Media",
        name: "Loki",
        icon: "mdi-file-document-multiple-outline",
        href: "https://logs.example.com",
        description: "Log aggregation",
      },
    },
    "svc-infra-prom": {
      homepage: {
        group: "Tools",
        name: "Prometheus",
        icon: "prometheus.svg",
        href: "https://metrics.example.com",
        description: "Metrics & alerts",
      },
    },
    "svc-infra-postgres": {
      homepage: {
        group: "Infra",
        name: "Postgres",
        icon:
          "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/postgres.svg",
        href: "https://db.example.com",
        description: "Transactional database",
      },
    },
  };
}

function denseHomepageOverrides() {
  return {
    "svc-prod-api": {
      homepage: {
        group: "Brain",
        name: "Acme API",
        icon: "si-github",
        href: "https://api.example.com",
        description: "API gateway & auth",
      },
    },
    "svc-prod-web": {
      homepage: {
        group: "Brain",
        name: "Web Console",
        icon: "mdi-monitor-dashboard",
        href: "https://web.example.com",
        description: "Primary admin console",
      },
    },
    "svc-prod-worker": {
      homepage: {
        group: "Ops",
        name: "Background Jobs",
        icon: "mdi-cog-refresh-outline",
        href: null,
        description: "Queue workers & cron",
      },
    },
    "svc-infra-loki": {
      homepage: {
        group: "Media",
        name: "Loki",
        icon: "mdi-file-document-multiple-outline",
        href: "https://logs.example.com",
        description: "Log aggregation",
      },
    },
    "svc-infra-prom": {
      homepage: {
        group: "Tools",
        name: "Prometheus",
        icon: "prometheus.svg",
        href: "https://metrics.example.com",
        description: "Metrics & alerts",
      },
    },
    "svc-infra-postgres": {
      homepage: {
        group: "Data",
        name: "Postgres",
        icon: "postgres.svg",
        href: "https://db.example.com",
        description: "Transactional database",
      },
    },
  };
}

function auditProofHomepageOverrides() {
  return {
    "svc-prod-api": {
      homepage: {
        group: "Brain",
        name: "Acme API",
        icon: "si-github",
        href: "https://api.example.com",
        description: "API gateway & auth",
      },
    },
    "svc-prod-web": {
      homepage: {
        group: "Brain",
        name: "Web Console",
        icon: "mdi-monitor-dashboard",
        href: "https://web.example.com",
        description: "Primary admin console",
      },
    },
    "svc-prod-worker": {
      homepage: {
        group: "Ops",
        name: "Background Jobs",
        icon: "sh-home-assistant.png",
        href: null,
        description: "Queue workers & cron",
      },
    },
    "svc-infra-loki": {
      homepage: {
        group: "Media",
        name: "Loki",
        icon: "nested/unsafe.svg",
        href: "https://logs.example.com",
        description: "Log aggregation",
      },
    },
    "svc-infra-prom": {
      homepage: {
        group: "Tools",
        name: "Prometheus",
        icon: "prometheus.svg",
        href: "https://metrics.example.com",
        description: "Metrics & alerts",
      },
    },
    "svc-infra-postgres": {
      homepage: {
        group: "Data",
        name: "Postgres",
        icon:
          "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/postgres.svg",
        href: "https://db.example.com",
        description: "Transactional database",
      },
    },
  };
}

function findCardByText(canvasElement: HTMLElement, text: string) {
  return serviceCards(canvasElement).find((card) =>
    card.textContent?.includes(text),
  );
}

function cachedNavSnapshot(generatedAt = new Date().toISOString()): HomepageNavSnapshot {
  return {
    version: 1,
    generatedAt,
    cards: [
      {
        id: "cached-acme-api",
        stackId: "stack-prod",
        stackName: "prod",
        serviceId: "svc-prod-api",
        serviceName: "api",
        imageRef: "ghcr.io/acme/api:5.2.1",
        groupName: "Cached Brain",
        title: "Cached Acme API",
        description: "Cached API gateway",
        href: "https://cached-api.example.com",
        icon: "si-github",
        status: "updatable",
        isDockrev: false,
      },
      {
        id: "cached-prom",
        stackId: "stack-infra",
        stackName: "infra",
        serviceId: "svc-infra-prom",
        serviceName: "prometheus",
        imageRef: "quay.io/prometheus/prometheus:v2.52.0",
        groupName: "Cached Tools",
        title: "Cached Prometheus",
        description: "Cached metrics",
        href: "https://cached-metrics.example.com",
        icon: "prometheus.svg",
        status: "ok",
        isDockrev: false,
      },
    ],
  };
}

function cachedResourceSnapshot(
  generatedAt = new Date(Date.now() - 10 * 60 * 1000).toISOString(),
): HomepageResourceSummarySnapshot {
  return {
    version: 1,
    generatedAt,
    overview: {
      enabled: true,
      window: "1h",
      generatedAt,
      staleAfterSeconds: 60,
      services: [
        {
          serviceId: "svc-prod-api",
          sampledAt: generatedAt,
          cpuPercent: 42,
          memUsedBytes: 512 * 1024 * 1024,
          memLimitBytes: 1024 * 1024 * 1024,
          netRxRateBps: 2048,
          netTxRateBps: 4096,
          stale: false,
          sampleCount: 12,
        },
      ],
    },
  };
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
      canvasElement.querySelector(".topbar .homepageHeaderContent"),
      "expected overview search to render in the global shell header",
    );
    expectStory(
      canvasElement.querySelector(".homepageMobileNavModule .homepageTopStrip"),
      "expected mobile overview strip to live in the page navigation module",
    );
    expectStory(
      !canvasElement.querySelector(".homepageMobileNavModule .homepageOverviewSearchShell"),
      "expected mobile navigation module to keep search out of the resource strip",
    );
    expectStory(
      canvasElement.querySelector(".mobileMenuEmbeddedContent .homepageDrawerSearchSlot"),
      "expected hamburger menu to keep overview search near the top of the drawer",
    );
    expectStory(
      canvasElement.querySelector(".mobileMenuEmbeddedContent .homepageDrawerBottomSummary"),
      "expected hamburger menu to keep resource summary at the drawer bottom",
    );
    expectStory(
      canvasElement.querySelectorAll(".topbar .homepageTopMetric").length ===
        4,
      "expected desktop top strip to expose four resource metrics",
    );
    expectStory(
      canvasElement.querySelectorAll(
        ".homepageMobileNavModule .homepageTopMetric",
      ).length === 4,
      "expected mobile navigation module to expose four resource metrics",
    );
    expectStory(
      canvasElement.querySelector(".homepageOverviewSearchShell"),
      "expected overview page to render an integrated search shell",
    );
    expectStory(
      canvasElement.querySelector('h1.srOnly')?.textContent === "服务导航",
      "expected overview page to expose a hidden page heading",
    );
    expectStory(
      canvasElement.querySelector<HTMLInputElement>(
        'input[type="search"][aria-label="搜索服务入口"]',
      ),
      "expected overview search input to expose a stable accessible label",
    );
    expectStory(
      canvasElement.querySelector('button[aria-label="刷新服务列表"]'),
      "expected refresh top action to keep an accessible name when labels collapse",
    );
    expectStory(
      canvasElement.querySelector('button[aria-label="立即扫描更新"]'),
      "expected scan top action to keep an accessible name when labels collapse",
    );
    expectStory(
      canvasElement.querySelector(".topbar .homepageHeaderSearchToggle"),
      "expected overview header search to provide a mobile collapsed button",
    );
    expectStory(
      !canvasElement.querySelector(".topbar .homepageClock"),
      "expected current time to stay out of the global shell header",
    );
    expectStory(
      canvasElement.querySelector(".sidebar .homepageSidebarClock"),
      "expected overview current time to render inside the left navigation",
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

export const CachedInstantNavigation: Story = {
  parameters: {
    dockrevApiScenario: "overview-homepage-slow-refresh",
    dockrevHomepageNavSnapshot: cachedNavSnapshot(),
    dockrevHomepageResourceSummarySnapshot: cachedResourceSnapshot(),
    dockrevServiceOverridesById: defaultHomepageOverrides(),
  },
  render: renderOverview(),
  play: async ({ canvasElement }) => {
    await sleep(80);

    expectStory(
      findCardByText(canvasElement, "Cached Acme API"),
      "cached launcher card should be visible before slow stack details return",
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
  },
  render: renderOverview(),
  play: async ({ canvasElement }) => {
    await sleep(260);

    expectStory(
      canvasElement.textContent?.includes("资源指标暂不可用"),
      "expected resource overview fetch failures to be visible",
    );
    const badges = Array.from(
      canvasElement.querySelectorAll<HTMLElement>(".homepageServiceStateBadge"),
    );
    expectStory(
      badges.some((badge) => badge.textContent?.includes("NO DATA")),
      "expected metric fetch failures to degrade cards to NO DATA instead of HEALTHY",
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
        ?.textContent?.includes("STALE"),
    );
    expectStory(staleCard, "expected stale resource samples to be surfaced");
    expectStory(
      staleCard.querySelectorAll(".homepageServiceMetricValue").length === 4,
      "expected stale cards to keep metric cells stable",
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

    expectStory(
      canvasElement.querySelector(".homepageDashboardGrid"),
      "expected mobile evidence story to expose the dashboard grid",
    );
    expectStory(
      canvasElement.querySelector(".homepageMobileNavModule .homepageTopStrip"),
      "expected mobile evidence story to render resource controls inside the navigation module",
    );
    expectStory(
      !canvasElement.querySelector(".homepageMobileNavModule .homepageOverviewSearchShell"),
      "expected mobile evidence story to keep search collapsed in the header",
    );
    expectStory(
      canvasElement.querySelector(".topbar .homepageHeaderSearchToggle"),
      "expected mobile evidence story to expose the collapsed header search button",
    );
    expectStory(
      !canvasElement.querySelector(".homepageMobileNavModule .homepageClock"),
      "expected mobile page navigation module to keep time out of the page header area",
    );
    expectStory(
      canvasElement.querySelector(".mobileMenuEmbeddedContent .homepageDrawerSearchSlot"),
      "expected hamburger menu to own the mobile search control",
    );
    expectStory(
      canvasElement.querySelector(".mobileMenuEmbeddedContent .homepageDrawerBottomSummary"),
      "expected hamburger menu to own the mobile resource summary",
    );
    expectStory(
      serviceCards(canvasElement).length >= 4,
      "expected mobile evidence story to render service cards",
    );
  },
};

export const IconKinds: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceOverridesById: {
      "svc-prod-api": {
        homepage: {
          group: "Developer",
          name: "Acme API",
          icon: "si-github",
          href: "https://api.example.com",
          description: "Primary API gateway",
        },
      },
      "svc-prod-web": {
        homepage: {
          group: "Frontend",
          name: "Web Console",
          icon: "mdi-monitor-dashboard",
          href: "https://web.example.com",
          description: "User-facing dashboard",
        },
      },
      "svc-infra-loki": {
        homepage: {
          group: "Monitoring",
          name: "Loki",
          icon: "sh-home-assistant.png",
          href: "https://logs.example.com",
          description: "Centralized logs",
        },
      },
      "svc-infra-prom": {
        homepage: {
          group: "Monitoring",
          name: "Prometheus",
          icon: "prometheus.svg",
          href: "https://metrics.example.com",
          description: "Metrics and alerting",
        },
      },
      "svc-infra-postgres": {
        homepage: {
          group: "Data",
          name: "Postgres",
          icon: "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/postgres.svg",
          href: "https://db.example.com",
          description: "Primary relational database",
        },
      },
    },
  },
  render: renderOverview(),
  play: async ({ canvasElement }) => {
    await sleep(260);

    const findCard = (text: string) =>
      serviceCards(canvasElement).find((card) =>
        card.textContent?.includes(text),
      );

    expectStory(
      findCard("Acme API")
        ?.querySelector(".homepageServiceIcon")
        ?.getAttribute("data-icon-kind") === "si",
      "expected Acme API card to use simple-icons parsing",
    );
    expectStory(
      findCard("Acme API")
        ?.querySelector(".homepageServiceIcon")
        ?.getAttribute("data-icon-src")
        ?.includes("/api/homepage-icons/iconify/simple-icons/github.svg?color=%23dbeafe") === true,
      "expected simple-icons monochrome icons to use the local proxy with the default light tint",
    );
    expectStory(
      findCard("Web Console")
        ?.querySelector(".homepageServiceIcon")
        ?.getAttribute("data-icon-kind") === "mdi",
      "expected Web Console card to use mdi parsing",
    );
    expectStory(
      findCard("Web Console")
        ?.querySelector(".homepageServiceIcon")
        ?.getAttribute("data-icon-src")
        ?.includes("/api/homepage-icons/iconify/mdi/monitor-dashboard.svg?color=%23dbeafe") === true,
      "expected mdi monochrome icons to use the local proxy with the default light tint",
    );
    expectStory(
      findCard("Loki")
        ?.querySelector(".homepageServiceIcon")
        ?.getAttribute("data-icon-kind") === "sh",
      "expected Loki card to use selfh.st parsing",
    );
    expectStory(
      findCard("Prometheus")
        ?.querySelector(".homepageServiceIcon")
        ?.getAttribute("data-icon-kind") === "dashboard",
      "expected Prometheus card to use dashboard-icons parsing",
    );
    expectStory(
      findCard("Postgres")
        ?.querySelector(".homepageServiceIcon")
        ?.getAttribute("data-icon-kind") === "url",
      "expected Postgres card to use absolute URL parsing",
    );
  },
};

export const UnsafeHomepageHrefFallsBack: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceOverridesById: {
      "svc-prod-api": {
        homepage: {
          group: "Developer",
          name: "Acme API",
          icon: "si-github",
          href: "javascript:alert(1)",
          description: "Primary API gateway",
        },
      },
    },
  },
  render: renderOverview(),
  play: async ({ canvasElement }) => {
    await sleep(260);

    const apiCard = serviceCards(canvasElement).find((card) =>
      card.textContent?.includes("Acme API"),
    );
    expectStory(
      !apiCard,
      "unsafe homepage href should remove the service from the launcher",
    );
    expectStory(
      !canvasElement.textContent?.includes("javascript:alert"),
      "unsafe homepage href should not leak into visible card text",
    );
  },
};
