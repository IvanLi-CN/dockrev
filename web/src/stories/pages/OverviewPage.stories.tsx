import type { Meta, StoryObj } from "@storybook/react";
import { OverviewPage } from "../../pages/OverviewPage";
import { PageHarness } from "../mocks/PageHarness";
import { withDockrevMockApi } from "../mocks/withDockrevMockApi";

const meta: Meta<typeof OverviewPage> = {
  title: "Pages/OverviewPage",
  component: OverviewPage,
  decorators: [withDockrevMockApi],
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

function renderOverview(pageSubtitle: string): Story["render"] {
  return () => (
    <PageHarness
      route={{ name: "overview" }}
      title="概览"
      pageSubtitle={pageSubtitle}
    >
      {({ onLastScanHint, onTopActions }) => (
        <OverviewPage
          onLastScanHint={onLastScanHint}
          onTopActions={onTopActions}
        />
      )}
    </PageHarness>
  );
}

export const Default: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceOverridesById: {
      "svc-prod-api": {
        homepage: {
          group: "Applications",
          name: "Acme API",
          icon: "si-github",
          href: "https://api.example.com",
          description: "API gateway & auth",
        },
      },
      "svc-prod-web": {
        homepage: {
          group: "Applications",
          name: "Web Console",
          icon: "mdi-monitor-dashboard",
          href: "https://web.example.com",
          description: "Primary admin console",
        },
      },
      "svc-prod-worker": {
        homepage: {
          group: "Applications",
          name: "Background Jobs",
          icon: "mdi-cog-refresh-outline",
          href: null,
          description: "Queue workers & cron",
        },
      },
      "svc-infra-loki": {
        homepage: {
          group: "Platform",
          name: "Loki",
          icon: "mdi-file-document-multiple-outline",
          href: "https://logs.example.com",
          description: "Log aggregation",
        },
      },
      "svc-infra-prom": {
        homepage: {
          group: "Platform",
          name: "Prometheus",
          icon: "prometheus.svg",
          href: "https://metrics.example.com",
          description: "Metrics & alerts",
        },
      },
      "svc-infra-postgres": {
        homepage: {
          group: "Platform",
          name: "Postgres",
          icon:
            "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/postgres.svg",
          href: "https://db.example.com",
          description: "Transactional database",
        },
      },
    },
  },
  render: renderOverview(
    "Homepage 兼容导航：统一网格卡片 + 更新丝带 + 点开查看详情",
  ),
  play: async ({ canvasElement }) => {
    await sleep(220);

    expectStory(
      !canvasElement.textContent?.includes("服务导航"),
      "expected overview page to remove the legacy service-navigation summary card",
    );
    expectStory(
      Array.from(canvasElement.querySelectorAll("button")).some((button) =>
        button.textContent?.includes("搜索"),
      ),
      "expected overview page to render an explicit search button",
    );
    expectStory(
      canvasElement.querySelector(".homepageOverviewSearchShell"),
      "expected overview page to render an integrated search shell",
    );
    const searchShell = canvasElement.querySelector<HTMLElement>(
      ".homepageOverviewSearchShell",
    );
    expectStory(searchShell, "expected overview search shell element");
    const searchShellRect = searchShell.getBoundingClientRect();
    const canvasRect = canvasElement.getBoundingClientRect();
    expectStory(
      searchShellRect.width >= canvasRect.width * 0.72,
      `expected overview search shell to use near full-row width, got ${Math.round(searchShellRect.width)} / ${Math.round(canvasRect.width)}`,
    );
    expectStory(
      canvasElement.querySelector(".homepageOverviewSearchButtonIcon"),
      "expected overview search button to include a search icon",
    );
    expectStory(
      !canvasElement.textContent?.includes("结果 6/6"),
      "expected overview page to hide the legacy result counter",
    );

    const cards = Array.from(
      canvasElement.querySelectorAll<HTMLAnchorElement>(".homepageServiceCard"),
    );
    expectStory(cards.length >= 4, "expected homepage cards to render");

    const sizes = cards.map((card) => {
      const rect = card.getBoundingClientRect();
      return `${Math.round(rect.width)}x${Math.round(rect.height)}`;
    });
    expectStory(
      new Set(sizes).size === 1,
      `expected homepage cards to use a uniform grid size, got ${sizes.join(", ")}`,
    );
    expectStory(
      !canvasElement.textContent?.includes("新窗口"),
      "expected homepage cards to avoid the new-window pill text",
    );
  },
};

export const SearchAndFallback: Story = {
  parameters: { dockrevApiScenario: "multi-stack-mixed" },
  render: renderOverview(
    "回归：未配置 Homepage 标签的服务仍需兜底展示并可搜索",
  ),
  play: async ({ canvasElement }) => {
    await sleep(220);

    const allCards = Array.from(
      canvasElement.querySelectorAll<HTMLAnchorElement>(".homepageServiceCard"),
    );
    expectStory(
      allCards.length >= 4,
      "expected homepage overview cards to render",
    );

    const workerCard = allCards.find((card) =>
      card.textContent?.includes("worker"),
    );
    expectStory(workerCard, "expected fallback worker card to render");
    expectStory(
      workerCard.getAttribute("target") === "_blank",
      "fallback worker card should open in a new tab",
    );
    expectStory(
      workerCard
        .getAttribute("href")
        ?.includes("/services/stack-prod/svc-prod-worker"),
      "fallback worker card should link to the Dockrev service detail route",
    );
    expectStory(
      workerCard.querySelector(".homepageServiceRibbon")?.textContent?.includes(
        "被阻止",
      ) === true,
      "fallback worker card should show the update-status ribbon",
    );

    const searchInput = canvasElement.querySelector<HTMLInputElement>(
      'input[placeholder*="搜索分组"]',
    );
    expectStory(searchInput, "expected overview search input");
    const searchButton = Array.from(
      canvasElement.querySelectorAll<HTMLButtonElement>("button"),
    ).find((button) => button.textContent?.includes("搜索"));
    expectStory(searchButton, "expected overview search button");
    setInputValue(searchInput, "worker");
    searchButton.click();
    await sleep(40);
    expectStory(
      searchButton
        .querySelector(".homepageOverviewSearchButtonIcon")
        ?.classList.contains("inlineIconSpinning") === true,
      "expected search icon to spin while applying the query",
    );
    await sleep(240);

    const filteredCards = Array.from(
      canvasElement.querySelectorAll<HTMLAnchorElement>(".homepageServiceCard"),
    );
    expectStory(
      filteredCards.length === 1,
      "search should filter overview cards down to the fallback worker entry",
    );
    expectStory(
      filteredCards[0].textContent?.includes("worker"),
      "filtered overview card should be worker",
    );

    setInputValue(searchInput, "ghcr.io/acme/api");
    searchButton.click();
    await sleep(260);

    const imageFilteredCards = Array.from(
      canvasElement.querySelectorAll<HTMLAnchorElement>(".homepageServiceCard"),
    );
    expectStory(
      imageFilteredCards.length === 1,
      "search should still match homepage cards by image ref",
    );
    expectStory(
      imageFilteredCards[0].textContent?.includes("Acme API"),
      "image-ref search should keep the Acme API card visible",
    );

    setInputValue(searchInput, "miniflux");
    searchButton.click();
    await sleep(260);

    const noUpdateCard = Array.from(
      canvasElement.querySelectorAll<HTMLAnchorElement>(".homepageServiceCard"),
    )[0];
    expectStory(
      !noUpdateCard.querySelector(".homepageServiceRibbon"),
      "cards without updates should not render a ribbon",
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
  render: renderOverview(
    "图标解析：si / mdi / sh / dashboard-icons / absolute URL",
  ),
  play: async ({ canvasElement }) => {
    await sleep(220);

    const findCard = (text: string) =>
      Array.from(
        canvasElement.querySelectorAll<HTMLAnchorElement>(
          ".homepageServiceCard",
        ),
      ).find((card) => card.textContent?.includes(text));

    expectStory(
      findCard("Acme API")
        ?.querySelector(".homepageServiceIcon")
        ?.getAttribute("data-icon-kind") === "si",
      "expected Acme API card to use simple-icons parsing",
    );
    expectStory(
      findCard("Acme API")
        ?.querySelector<HTMLImageElement>(".homepageServiceIconImage")
        ?.src.includes("color=%23dbeafe") === true,
      "expected simple-icons monochrome icons to use the default light tint",
    );
    expectStory(
      findCard("Web Console")
        ?.querySelector(".homepageServiceIcon")
        ?.getAttribute("data-icon-kind") === "mdi",
      "expected Web Console card to use mdi parsing",
    );
    expectStory(
      findCard("Web Console")
        ?.querySelector<HTMLImageElement>(".homepageServiceIconImage")
        ?.src.includes("color=%23dbeafe") === true,
      "expected mdi monochrome icons to use the default light tint",
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
  render: renderOverview("回归：不安全 Homepage href 必须回退到内部详情页"),
  play: async ({ canvasElement }) => {
    await sleep(220);

    const apiCard = Array.from(
      canvasElement.querySelectorAll<HTMLAnchorElement>(".homepageServiceCard"),
    ).find((card) => card.textContent?.includes("Acme API"));
    expectStory(apiCard, "expected Acme API card to render");
    expectStory(
      apiCard
        .getAttribute("href")
        ?.includes("/services/stack-prod/svc-prod-api"),
      "unsafe homepage href should fall back to the Dockrev service detail route",
    );
    expectStory(
      !apiCard.textContent?.includes("详情"),
      "homepage cards should not render the legacy detail pill text",
    );
  },
};
