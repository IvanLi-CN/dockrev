import { currentRoutePathname } from "../../routes";
import { versionReleaseNotes } from "./serviceDetailPageStoryFixtures";
import {
  findSectionCard,
  findTab,
  findVersionAction,
  findVersionCard,
  render,
  type ServiceDetailStory,
  visibleVersionCards,
} from "./serviceDetailStoryShared";
import { expectNearlyEqual, expectStory, findButton, normalizeText, waitForCondition } from "./storyAssertions";

function versionsSurface(root: ParentNode): HTMLElement | null {
  return root.querySelector<HTMLElement>('[data-service-versions="true"]');
}

function versionsViewport(root: ParentNode): HTMLElement | null {
  return root.querySelector<HTMLElement>(".serviceVersionsScrollViewport");
}

function versionsIndexViewport(root: ParentNode): HTMLElement | null {
  return root.querySelector<HTMLElement>(".serviceVersionsIndexViewport");
}

function versionsIndexItem(root: ParentNode, tagName: string): HTMLButtonElement | null {
  return root.querySelector<HTMLButtonElement>(
    `[data-service-versions-index-item="true"][data-release-tag="${tagName}"]`,
  );
}

function selectedIndexTag(root: ParentNode): string | null {
  return (
    root
      .querySelector<HTMLElement>('[data-service-versions-index-selected="true"]')
      ?.getAttribute("data-release-tag") ?? null
  );
}

function centeredVersionTag(root: ParentNode, viewport: HTMLElement | null): string | null {
  if (!viewport) return null;
  const viewportRect = viewport.getBoundingClientRect();
  const viewportCenter = viewportRect.top + viewportRect.height / 2;
  const cards = visibleVersionCards(root).filter((card) => {
    const rect = card.getBoundingClientRect();
    return rect.bottom > viewportRect.top && rect.top < viewportRect.bottom;
  });
  if (cards.length === 0) return null;
  return cards
    .slice()
    .sort((left, right) => {
      const leftCenter = left.getBoundingClientRect().top + left.getBoundingClientRect().height / 2;
      const rightCenter = right.getBoundingClientRect().top + right.getBoundingClientRect().height / 2;
      return Math.abs(leftCenter - viewportCenter) - Math.abs(rightCenter - viewportCenter);
    })[0]
    ?.getAttribute("data-release-tag") ?? null;
}

function visibleCount(root: ParentNode, attr: string): number {
  return Number(versionsSurface(root)?.getAttribute(attr) ?? "0");
}

export const VersionsSection: ServiceDetailStory = {
  parameters: {
    dockrevApiScenario: "service-detail-history-rollback-action",
    dockrevGitHubReleasesByServiceId: {
      "svc-prod-api": {
        authMode: "anonymous",
        repo: { fullName: "acme/api", htmlUrl: "https://github.com/acme/api" },
        items: versionReleaseNotes,
      },
    },
  },
  render: render("stack-prod", "svc-prod-api", "versions", "桌面端版本子页以双虚拟列表展示，并保留统一动作守卫。"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "versions")));
    await waitForCondition(() => Boolean(findVersionCard(canvasElement, "5.2.1")));
    await waitForCondition(() => selectedIndexTag(canvasElement) === "5.2.1");

    const surface = versionsSurface(canvasElement);
    const viewport = versionsViewport(canvasElement);
    const indexViewport = versionsIndexViewport(canvasElement);
    const currentCard = findVersionCard(canvasElement, "5.2.1");
    const candidateCard = findVersionCard(canvasElement, "5.2.3");
    const rollbackCard = findVersionCard(canvasElement, "5.2.0");
    const updateCandidate = findVersionAction(canvasElement, "update", "5.2.3");
    const updateDisabled = findVersionAction(canvasElement, "update", "5.4.4");
    const rollbackTarget = findVersionAction(canvasElement, "rollback", "5.2.0");
    const rollbackHint = findVersionAction(canvasElement, "rollback", "5.2.2");
    const githubLink = canvasElement.querySelector<HTMLAnchorElement>(
      '.serviceVersionsHeaderControls [data-link-icon="github"]',
    );
    const octoRillLink = canvasElement.querySelector<HTMLAnchorElement>(
      '.serviceVersionsHeaderControls [data-link-icon="octorill"]',
    );

    expectStory(currentRoutePathname() === "/services/stack-prod/svc-prod-api/versions", "versions deep link missing");
    expectStory(findTab(canvasElement, "versions")?.getAttribute("data-state") === "active", "versions tab should be active");
    expectStory(surface?.getAttribute("data-service-versions-layout") === "desktop", "desktop story should expose the desktop split layout");
    expectStory(Boolean(indexViewport), "desktop versions index viewport missing");
    expectStory(Boolean(currentCard), "current version card missing");
    expectStory(Boolean(candidateCard), "candidate version card missing");
    expectStory(Boolean(rollbackCard), "rollback target card missing");
    expectStory(
      Math.abs((indexViewport?.getBoundingClientRect().width ?? 0) - 220) <= 6,
      "desktop version index should reserve a stable 220px rail",
    );
    expectStory(
      canvasElement.querySelectorAll('.serviceVersionsHeaderControls [data-link-kind="repo"]').length === 2,
      "versions header should only expose the two repository-level release links",
    );
    expectStory(
      canvasElement.querySelectorAll(".serviceVersionsHeaderControls .pill").length === 0,
      "versions header should remove the old repository/version chips",
    );
    expectStory(githubLink?.href === "https://github.com/acme/api/releases", "GitHub releases link mismatch");
    expectStory(octoRillLink?.href === "https://octo.example.com/acme/api/releases", "OctoRill releases link mismatch");
    expectStory(
      visibleCount(canvasElement, "data-service-versions-total-count") === 20,
      "versions surface should only preload the first page until the user nears the tail",
    );
    expectStory(
      visibleCount(canvasElement, "data-service-versions-list-visible-count") > 0 &&
        visibleCount(canvasElement, "data-service-versions-list-visible-count") <
          visibleCount(canvasElement, "data-service-versions-total-count"),
      "versions cards should stay virtualized",
    );
    expectStory(
      visibleCount(canvasElement, "data-service-versions-index-visible-count") > 0 &&
        visibleCount(canvasElement, "data-service-versions-index-visible-count") <
          visibleCount(canvasElement, "data-service-versions-total-count"),
      "versions index should stay virtualized",
    );
    expectStory(
      Math.abs(
        ((currentCard?.getBoundingClientRect().top ?? 0) + (currentCard?.getBoundingClientRect().height ?? 0) / 2) -
          ((viewport?.getBoundingClientRect().top ?? 0) + (viewport?.getBoundingClientRect().height ?? 0) / 2),
      ) < 200,
      "current deployed version should be centered inside the versions viewport on first render",
    );
    expectStory(
      selectedIndexTag(canvasElement) === "5.2.1" &&
        centeredVersionTag(canvasElement, viewport) === "5.2.1",
      "initial centered card should drive the matching index highlight",
    );
    const currentAside = currentCard?.querySelector<HTMLElement>('[data-service-version-card-aside="true"]');
    expectStory(Boolean(currentAside), "read-only current version cards should still reserve the fixed aside rail");
    expectStory(
      getComputedStyle(candidateCard ?? canvasElement).gridTemplateColumns.split(" ").filter(Boolean).length === 3,
      "actionable desktop version cards should keep the fixed third rail",
    );
    const asideWidths = [currentAside, candidateCard?.querySelector<HTMLElement>('[data-service-version-card-aside="true"]'), rollbackCard?.querySelector<HTMLElement>('[data-service-version-card-aside="true"]')]
      .filter((aside): aside is HTMLElement => Boolean(aside))
      .map((aside) => aside.getBoundingClientRect().width);
    expectStory(asideWidths.length === 3, "expected placeholder and actionable rails for width comparison");
    expectStory(
      asideWidths.every((width) => width >= 228 && width <= 252),
      "desktop aside rail should stay within the tightened fixed width budget",
    );
    expectNearlyEqual(asideWidths[0] ?? 0, asideWidths[1] ?? 0, 1.5, "desktop placeholder and action rails should keep equal width");
    expectNearlyEqual(asideWidths[1] ?? 0, asideWidths[2] ?? 0, 1.5, "desktop action rails should keep equal width");
    expectStory(updateCandidate && !updateCandidate.disabled, "candidate version should expose an enabled update action");
    expectStory(Boolean(updateDisabled?.disabled), "newer non-candidate release should render a disabled update action");
    expectStory(Boolean(rollbackTarget && !rollbackTarget.disabled), "rollback target version should expose an enabled rollback action");
    expectStory(Boolean(rollbackHint && !rollbackHint.disabled), "historically deployed old versions should keep an explanatory rollback entry");
    expectStory(
      !normalizeText(findVersionCard(canvasElement, "5.2.2")?.textContent).includes("不一定是当前可执行 rollback target"),
      "historical releases should not render the noisy rollback-target disclaimer in the card aside",
    );
    expectStory(
      findVersionCard(canvasElement, "5.2.0")?.getAttribute("data-version-card-older") === "true",
      "older comparable releases should render the de-emphasized state",
    );

    findButton(canvasElement, "原文")?.click();
    await waitForCondition(
      () => versionsSurface(canvasElement)?.getAttribute("data-service-versions-view") === "original",
    );
    const expandButton = Array.from(currentCard?.querySelectorAll<HTMLButtonElement>("button") ?? []).find(
      (button) => normalizeText(button.textContent) === "展开",
    );
    expectStory(expandButton, "long release notes should default to the collapsed state");
    expectStory(
      !normalizeText(currentCard?.textContent).includes("故意超过十行"),
      "collapsed release notes should hide lines beyond the first ten",
    );
    expandButton?.click();
    await waitForCondition(() => normalizeText(currentCard?.textContent).includes("故意超过十行"));

    rollbackHint?.click();
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => doc.body.textContent?.includes("不会直接创建回滚任务") ?? false);
    findButton(doc, "知道了")?.click();
    await waitForCondition(() => !(doc.body.textContent?.includes("不会直接创建回滚任务") ?? false));

    const initialSelected = selectedIndexTag(canvasElement);
    viewport?.scrollTo({ top: Math.max(0, (viewport?.scrollTop ?? 0) + 280) });
    await waitForCondition(() => {
      const centeredTag = centeredVersionTag(canvasElement, viewport);
      const selectedTag = selectedIndexTag(canvasElement);
      return Boolean(centeredTag) && centeredTag === selectedTag && selectedTag !== initialSelected;
    });

    indexViewport?.scrollTo({ top: indexViewport.scrollHeight });
    await waitForCondition(
      () => visibleCount(canvasElement, "data-service-versions-total-count") === 40,
      5000,
    );
    const pageTwoIndexItem = versionsIndexItem(canvasElement, "5.0.3");
    expectStory(pageTwoIndexItem, "page-two index item should appear after index tail pagination");
    pageTwoIndexItem?.click();
    await waitForCondition(() => selectedIndexTag(canvasElement) === "5.0.3");
    await waitForCondition(() => centeredVersionTag(canvasElement, viewport) === "5.0.3");
    await waitForCondition(() => Boolean(findVersionCard(canvasElement, "5.0.3")));

    viewport?.scrollTo({ top: viewport.scrollHeight });
    await waitForCondition(
      () => visibleCount(canvasElement, "data-service-versions-total-count") === 45,
      5000,
    );
  },
};

export const VersionsSectionActionGuard: ServiceDetailStory = {
  parameters: {
    dockrevApiScenario: "dashboard-demo-slow-update",
    dockrevGitHubReleasesByServiceId: {
      "svc-prod-api": {
        authMode: "anonymous",
        repo: { fullName: "acme/api", htmlUrl: "https://github.com/acme/api" },
        items: versionReleaseNotes,
      },
    },
  },
  render: render("stack-prod", "svc-prod-api", "versions", "更新或回滚任务执行期间，同一服务的版本动作必须统一锁定。"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findVersionAction(canvasElement, "update", "5.2.3")));
    findVersionAction(canvasElement, "update", "5.2.3")?.click();

    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => doc.body.textContent?.includes("确认更新服务 api？") ?? false);
    findButton(doc, "执行更新")?.click();

    await waitForCondition(() => normalizeText(canvasElement.textContent).includes("当前服务已有更新任务在执行"));
    await waitForCondition(() => Boolean(globalThis.__DOCKREV_MOCK_DEBUG__?.lastUpdateRequest));

    const lastRequest = globalThis.__DOCKREV_MOCK_DEBUG__?.lastUpdateRequest as
      | Record<string, unknown>
      | null
      | undefined;
    expectStory(lastRequest?.targetTag === "5.2.1", "version-page update must still obey the current deployed targetTag contract");
    expectStory(
      typeof lastRequest?.targetDigest === "string" && String(lastRequest.targetDigest).endsWith("9f"),
      "version-page update must keep the existing candidate digest contract",
    );
    expectStory(Boolean(findVersionAction(canvasElement, "update", "5.2.3")?.disabled), "candidate update action should lock once a task is running");
    expectStory(Boolean(findVersionAction(canvasElement, "rollback", "5.2.0")?.disabled), "rollback actions should also lock while the service has an update task in flight");
    expectStory(Boolean(findButton(canvasElement, "查看任务")), "locked versions state should still expose the active job entrypoint");
  },
};

export const MobileVersionsSection: ServiceDetailStory = {
  parameters: {
    dockrevApiScenario: "service-detail-history-rollback-action",
    viewport: { defaultViewport: "mobile1" },
    dockrevGitHubReleasesByServiceId: {
      "svc-prod-api": {
        authMode: "anonymous",
        repo: { fullName: "acme/api", htmlUrl: "https://github.com/acme/api" },
        items: versionReleaseNotes,
      },
    },
  },
  render: render("stack-prod", "svc-prod-api", "versions", "移动端版本卡保持单列，无目录且不得横向溢出。"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findVersionCard(canvasElement, "5.2.1")));
    const surface = versionsSurface(canvasElement);
    const card = findVersionCard(canvasElement, "5.2.1");
    const factsGrid = card?.querySelector<HTMLElement>(".serviceVersionFacts");
    const primaryButton = findVersionAction(canvasElement, "update", "5.2.3");
    await waitForCondition(() => visibleVersionCards(canvasElement).length >= 2);
    const [firstVisibleCard, secondVisibleCard] = visibleVersionCards(canvasElement).sort(
      (left, right) => left.getBoundingClientRect().top - right.getBoundingClientRect().top,
    );
    const gridColumns = getComputedStyle(card ?? canvasElement).gridTemplateColumns.split(" ").filter(Boolean);
    const factsColumns = getComputedStyle(factsGrid ?? canvasElement).gridTemplateColumns.split(" ").filter(Boolean);

    expectStory(findTab(canvasElement, "versions")?.getAttribute("data-state") === "active", "mobile versions tab should stay active");
    expectStory(surface?.getAttribute("data-service-versions-layout") === "mobile", "mobile story should switch to the single-column layout");
    expectStory(!canvasElement.querySelector('[data-service-versions-index="true"]'), "mobile versions should hide the desktop version index");
    expectStory(
      visibleCount(canvasElement, "data-service-versions-index-visible-count") === 0,
      "mobile versions should not render index rows",
    );
    expectStory(gridColumns.length === 1, "mobile versions cards should collapse into a single-column layout");
    expectStory(factsColumns.length === 2, "mobile versions metadata should stay in a compact two-column facts grid");
    expectStory(Boolean(primaryButton), "mobile versions card should keep the action region");
    expectStory(
      surface?.scrollWidth != null &&
        surface.clientWidth > 0 &&
        surface.scrollWidth <= surface.clientWidth + 1,
      "mobile versions surface should not overflow horizontally",
    );
    expectStory(
      card?.scrollWidth != null && card.clientWidth > 0 && card.scrollWidth <= card.clientWidth + 1,
      "mobile versions cards should not overflow horizontally",
    );
    expectStory((primaryButton?.getBoundingClientRect().width ?? 0) > 160, "mobile versions actions should expand to a readable tap target width");
    expectStory(
      (secondVisibleCard?.getBoundingClientRect().top ?? 0) >=
        (firstVisibleCard?.getBoundingClientRect().bottom ?? 0) - 1,
      "mobile versions virtual rows should not overlap after dynamic height measurement",
    );
  },
};
