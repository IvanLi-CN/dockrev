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
  parameters: { dockrevApiScenario: "dashboard-demo" },
  render: renderOverview(
    "Homepage 兼容导航：分组卡片 + 新版本标记 + 新窗口打开",
  ),
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

    const searchInput = canvasElement.querySelector<HTMLInputElement>(
      'input[placeholder*="搜索分组"]',
    );
    expectStory(searchInput, "expected overview search input");
    setInputValue(searchInput, "worker");
    await sleep(140);

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
    await sleep(140);

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
      findCard("Web Console")
        ?.querySelector(".homepageServiceIcon")
        ?.getAttribute("data-icon-kind") === "mdi",
      "expected Web Console card to use mdi parsing",
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
      apiCard.textContent?.includes("详情"),
      "unsafe homepage href fallback should render the internal detail pill",
    );
  },
};
