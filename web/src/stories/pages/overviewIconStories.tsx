import type { StoryObj } from "@storybook/react";
import { OverviewPage } from "../../pages/OverviewPage";
import { PageHarness } from "../mocks/PageHarness";

type Story = StoryObj<typeof OverviewPage>;

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function expectStory(condition: unknown, message: string): asserts condition {
  if (!condition) throw new globalThis.Error(message);
}

function renderOverview(): Story["render"] {
  return () => (
    <PageHarness route={{ name: "overview" }} title="" runtimeMode={null}>
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
  return Array.from(canvasElement.querySelectorAll<HTMLElement>(".homepageServiceCard"));
}

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
    const findCard = (text: string) => serviceCards(canvasElement).find((card) => card.textContent?.includes(text));
    expectStory(findCard("Acme API")?.querySelector(".homepageServiceIcon")?.getAttribute("data-icon-kind") === "si", "expected Acme API card to use simple-icons parsing");
    expectStory(findCard("Acme API")?.querySelector(".homepageServiceIcon")?.getAttribute("data-icon-src")?.includes("/api/homepage-icons/iconify/simple-icons/github.svg?color=%23dbeafe") === true, "expected simple-icons monochrome icons to use the local proxy with the default light tint");
    expectStory(findCard("Web Console")?.querySelector(".homepageServiceIcon")?.getAttribute("data-icon-kind") === "mdi", "expected Web Console card to use mdi parsing");
    expectStory(findCard("Web Console")?.querySelector(".homepageServiceIcon")?.getAttribute("data-icon-src")?.includes("/api/homepage-icons/iconify/mdi/monitor-dashboard.svg?color=%23dbeafe") === true, "expected mdi monochrome icons to use the local proxy with the default light tint");
    expectStory(findCard("Loki")?.querySelector(".homepageServiceIcon")?.getAttribute("data-icon-kind") === "sh", "expected Loki card to use selfh.st parsing");
    expectStory(findCard("Prometheus")?.querySelector(".homepageServiceIcon")?.getAttribute("data-icon-kind") === "dashboard", "expected Prometheus card to use dashboard-icons parsing");
    expectStory(findCard("Postgres")?.querySelector(".homepageServiceIcon")?.getAttribute("data-icon-kind") === "url", "expected Postgres card to use absolute URL parsing");
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
    const apiCard = serviceCards(canvasElement).find((card) => card.textContent?.includes("Acme API"));
    expectStory(!apiCard, "unsafe homepage href should remove the service from the launcher");
    expectStory(!canvasElement.textContent?.includes("javascript:alert"), "unsafe homepage href should not leak into visible card text");
  },
};
