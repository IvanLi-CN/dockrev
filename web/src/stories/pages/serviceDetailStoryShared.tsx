import type { StoryObj } from "@storybook/react";
import { ServiceDetailPage } from "../../pages/ServiceDetailPage";
import type { Route } from "../../routes";
import { PageHarness } from "../mocks/PageHarness";
import { findButton, normalizeText } from "./storyAssertions";

export type ServiceDetailStory = StoryObj<typeof ServiceDetailPage>;
export type ServiceSection =
  | "overview"
  | "versions"
  | "history"
  | "monitoring"
  | "backup"
  | "logs"
  | "settings";

export function findActionButton(root: ParentNode, action: string, text: string): HTMLButtonElement | null {
  const scope = root.querySelector(`[data-service-detail-action="${action}"]`);
  if (!scope) return null;
  return findButton(scope, text);
}

export function findSectionCard(root: ParentNode, card: string): HTMLElement | null {
  return root.querySelector<HTMLElement>(`[data-service-detail-section-card="${card}"]`);
}

export function findTab(root: ParentNode, section: ServiceSection): HTMLButtonElement | null {
  return root.querySelector<HTMLButtonElement>(`[data-service-detail-tab="${section}"]`);
}

export function tabLabels(root: ParentNode): string[] {
  return Array.from(root.querySelectorAll<HTMLElement>("[data-service-detail-tab]")).map((tab) =>
    normalizeText(tab.textContent),
  );
}

export function findHistoryRowByJobId(root: ParentNode, jobId: string): HTMLElement | null {
  return (
    Array.from(root.querySelectorAll<HTMLElement>(".serviceOperationHistoryRow")).find((row) =>
      normalizeText(row.textContent).includes(jobId),
    ) ?? null
  );
}

export function findVersionCard(root: ParentNode, tagName: string): HTMLElement | null {
  return root.querySelector<HTMLElement>(`[data-service-version-card="true"][data-release-tag="${tagName}"]`);
}

export function findVersionAction(
  root: ParentNode,
  action: "update" | "rollback",
  tagName: string,
): HTMLButtonElement | null {
  return root.querySelector<HTMLButtonElement>(
    `[data-service-version-action="${action}"][data-release-tag="${tagName}"] button`,
  );
}

export function visibleVersionCards(root: ParentNode): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>('[data-service-version-card="true"]')).filter(
    (card) => card.getBoundingClientRect().height > 0,
  );
}

export function findLogRowContaining(root: ParentNode, text: string): HTMLElement | null {
  return (
    Array.from(root.querySelectorAll<HTMLElement>(".serviceLogRow")).find((row) =>
      normalizeText(row.textContent).includes(text),
    ) ?? null
  );
}

export function drawerText(doc: Document): string {
  return normalizeText(doc.querySelector(".settingsDrawerContent")?.textContent);
}

function routeFor(stackId: string, serviceId: string, section: ServiceSection = "overview"): Route {
  return section === "overview"
    ? { name: "service", stackId, serviceId }
    : { name: "service", stackId, serviceId, section };
}

export function render(
  stackId: string,
  serviceId: string,
  section: ServiceSection = "overview",
  storyConfig?: string | {
    sidebarCollapsed?: boolean;
    pageTitle?: string | null;
    pageSubtitle?: string | null;
  },
  options?: {
    sidebarCollapsed?: boolean;
    pageTitle?: string | null;
    pageSubtitle?: string | null;
  },
): ServiceDetailStory["render"] {
  const resolvedOptions =
    typeof storyConfig === "string"
      ? options
      : storyConfig;
  return () => (
    <PageHarness
      route={routeFor(stackId, serviceId, section)}
      sidebarCollapsed={resolvedOptions?.sidebarCollapsed ?? false}
    >
      {({ route, onTopActions, onLastScanHint, onPageTitle, onTopbarContent }) =>
        route.name === "service" ? (
          <ServiceDetailPage
            stackId={route.stackId}
            serviceId={route.serviceId}
            section={route.section}
            onLastScanHint={onLastScanHint}
            onTopActions={onTopActions}
            onPageTitle={onPageTitle}
            onTopbarContent={onTopbarContent}
          />
        ) : null
      }
    </PageHarness>
  );
}
