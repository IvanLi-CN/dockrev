import { currentRoutePathname } from "../../routes";
import { dockrevVersionReleaseNotes, versionReleaseNotes } from "./serviceDetailPageStoryFixtures";
import {
  findSectionCard,
  findTab,
  findVersionAction,
  findVersionCard,
  render,
  type ServiceDetailStory,
  visibleVersionCards,
} from "./serviceDetailStoryShared";
import { expectNearlyEqual, expectStory, findButton, findButtons, normalizeText, waitForCondition } from "./storyAssertions";

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

const dockrevDigest = (fill: string, last2: string) => `sha256:${fill.repeat(62)}${last2}`;

const dockrevServiceOverride = {
  name: "dockrev",
  image: {
    ref: "ghcr.io/ivanli-cn/dockrev:0.61.0",
    tag: "0.61.0",
    digest: dockrevDigest("6", "10"),
  },
  candidate: {
    tag: "0.62.0",
    digest: dockrevDigest("7", "20"),
    archMatch: "match",
    arch: ["linux/amd64"],
  },
  ignore: null,
} as const;

const dockrevSelfUpgradeStoryParameters = {
  dockrevApiScenario: "service-detail-history-rollback-action",
  dockrevServiceOverridesById: {
    "svc-prod-api": dockrevServiceOverride,
  },
  dockrevGitHubReleasesByServiceId: {
    "svc-prod-api": {
      authMode: "anonymous",
      repo: { fullName: "IvanLi-CN/dockrev", htmlUrl: "https://github.com/IvanLi-CN/dockrev" },
      items: dockrevVersionReleaseNotes,
    },
  },
} as const;

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
  render: render("stack-prod", "svc-prod-api", "versions", undefined, {
    pageTitle: null,
    pageSubtitle: null,
  }),
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
    const currentTitle = currentCard?.querySelector<HTMLElement>(".serviceVersionBodyTitle");
    const currentTag = currentCard?.querySelector<HTMLElement>(".serviceVersionTagText .mono");
    const currentMetaValue = currentCard?.querySelector<HTMLElement>(".serviceVersionFacts dd");
    const currentAside = currentCard?.querySelector<HTMLElement>('[data-service-version-card-aside="true"]');
    const candidateAside = candidateCard?.querySelector<HTMLElement>('[data-service-version-card-aside="true"]');
    const rollbackAside = rollbackCard?.querySelector<HTMLElement>('[data-service-version-card-aside="true"]');
    const markdownHeading = currentCard?.querySelector<HTMLElement>(".serviceVersionBody h2");
    const markdownList = currentCard?.querySelector<HTMLElement>(".serviceVersionBody ul");
    expectStory(
      Number.parseFloat(getComputedStyle(currentTitle ?? canvasElement).fontSize) >
        Number.parseFloat(getComputedStyle(currentMetaValue ?? canvasElement).fontSize),
      "release title should live in the reading column and stay visually above body metadata",
    );
    expectStory(
      normalizeText(currentTitle?.textContent).includes("Service detail release reading flow"),
      "current version card should render the release title inside the reading column",
    );
    expectStory(
      normalizeText(currentTag?.textContent) === "5.2.1",
      "version tag should stay in the left metadata rail instead of being replaced by the release title",
    );
    expectStory(
      normalizeText(markdownHeading?.textContent) === "What's Changed",
      "GitHub release markdown headings should render as structured heading elements",
    );
    expectStory(
      Boolean(markdownList?.querySelector("li")),
      "GitHub release markdown bullet lists should render as list items",
    );
    expectStory(Boolean(currentAside), "read-only current version cards should still reserve the fixed aside rail");
    expectStory(
      getComputedStyle(candidateCard ?? canvasElement).gridTemplateColumns.split(" ").filter(Boolean).length === 3,
      "actionable desktop version cards should keep the fixed third rail",
    );
    const asideWidths = [currentAside, candidateAside, rollbackAside]
      .filter((aside): aside is HTMLElement => Boolean(aside))
      .map((aside) => aside.getBoundingClientRect().width);
    expectStory(asideWidths.length === 3, "expected placeholder and actionable rails for width comparison");
    expectStory(
      asideWidths.every((width) => width >= 194 && width <= 202),
      "desktop aside rail should stay within the compact fixed width budget",
    );
    expectNearlyEqual(asideWidths[0] ?? 0, asideWidths[1] ?? 0, 1.5, "desktop placeholder and action rails should keep equal width");
    expectNearlyEqual(asideWidths[1] ?? 0, asideWidths[2] ?? 0, 1.5, "desktop action rails should keep equal width");
    expectStory(updateCandidate && !updateCandidate.disabled, "candidate version should expose an enabled update action");
    expectStory(Boolean(updateDisabled?.disabled), "newer non-candidate release should render a disabled update action");
    expectStory(Boolean(rollbackTarget && !rollbackTarget.disabled), "rollback target version should expose an enabled rollback action");
    expectStory(
      normalizeText(findVersionCard(canvasElement, "5.2.0")?.textContent).includes("来源备份") &&
        normalizeText(findVersionCard(canvasElement, "5.2.0")?.textContent).includes("2 个目标 · 17.6 MiB"),
      "rollback target card should surface the matched backup summary",
    );
    expectStory(!rollbackHint, "non-target historical releases should not expose any rollback entry");
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
    const changelogLink = currentCard?.querySelector<HTMLAnchorElement>('.serviceVersionBody a[href*="/compare/"]');
    expectStory(
      changelogLink?.href === "https://github.com/acme/api/compare/5.2.1-prev...5.2.1",
      "expanded release notes should keep the rendered changelog link",
    );

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
  render: render("stack-prod", "svc-prod-api", "versions", undefined, {
    pageTitle: null,
    pageSubtitle: null,
  }),
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

export const DockrevVersionsSelfUpgrade: ServiceDetailStory = {
  parameters: dockrevSelfUpgradeStoryParameters,
  render: render("stack-prod", "svc-prod-api", "versions", "Dockrev 版本页的候选卡必须走 supervisor 自我升级，而不是普通服务更新。"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findVersionCard(canvasElement, "0.61.0")));
    const doc = canvasElement.ownerDocument;
    const candidateAction = findVersionAction(canvasElement, "update", "0.62.0");
    const newerAction = findVersionAction(canvasElement, "update", "0.63.0");
    const topAction = findButtons(doc, "升级 Dockrev").find(
      (button) => !button.closest('[data-service-version-action="update"]'),
    );

    expectStory(currentRoutePathname() === "/services/stack-prod/svc-prod-api/versions", "dockrev versions deep link missing");
    expectStory(findTab(canvasElement, "versions")?.getAttribute("data-state") === "active", "dockrev versions tab should stay active");
    expectStory(Boolean(candidateAction && !candidateAction.disabled), "dockrev candidate card should expose an enabled self-upgrade action");
    expectStory(Boolean(newerAction?.disabled), "newer non-candidate dockrev release should stay disabled");
    expectStory(normalizeText(newerAction?.textContent).includes("仅候选可升级"), "non-candidate dockrev release should stop masquerading as a clickable self-upgrade button");
    expectStory(normalizeText(findVersionCard(canvasElement, "0.63.0")?.textContent).includes("这个版本不是当前候选；当前只能升级候选 0.62.0。"), "non-candidate dockrev release should explain the candidate-only upgrade truth directly");
    expectStory(normalizeText(findVersionCard(canvasElement, "0.62.0")?.textContent).includes("当前候选 0.62.0 已就绪；点击后进入 Dockrev 自我升级流程。"), "candidate dockrev release should explain the active self-upgrade handoff");
    expectStory(Boolean(topAction && !topAction.disabled), "top-level dockrev self-upgrade action should stay enabled alongside the candidate card");
    expectStory(!globalThis.__DOCKREV_MOCK_DEBUG__?.lastUpdateRequest, "dockrev self-upgrade story must start without an ordinary update request");

    candidateAction?.click();
    await waitForCondition(() => currentRoutePathname() === "/supervisor/");
    expectStory(!globalThis.__DOCKREV_MOCK_DEBUG__?.lastUpdateRequest, "dockrev candidate action must not trigger a normal service update request");
  },
};

export const DockrevVersionsSelfUpgradeVisual: ServiceDetailStory = {
  parameters: dockrevSelfUpgradeStoryParameters,
  render: render("stack-prod", "svc-prod-api", "versions", "Dockrev 版本页候选卡与顶部入口共享 supervisor 自我升级语义。"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findVersionCard(canvasElement, "0.62.0")));
    const doc = canvasElement.ownerDocument;
    const candidateAction = findVersionAction(canvasElement, "update", "0.62.0");
    const newerAction = findVersionAction(canvasElement, "update", "0.63.0");
    const topAction = findButtons(doc, "升级 Dockrev").find(
      (button) => !button.closest('[data-service-version-action="update"]'),
    );
    const newerCard = findVersionCard(canvasElement, "0.63.0");
    const newerAside = newerCard?.querySelector<HTMLElement>('[data-service-version-card-aside="true"]') ?? null;
    const newerBody = newerCard?.querySelector<HTMLElement>(".serviceVersionCardBody") ?? null;
    const newerButtonWidth = newerAction?.getBoundingClientRect().width ?? 0;
    const newerAsideWidth = newerAside?.getBoundingClientRect().width ?? 0;
    const newerBodyWidth = newerBody?.getBoundingClientRect().width ?? 0;

    expectStory(currentRoutePathname() === "/services/stack-prod/svc-prod-api/versions", "dockrev visual evidence must stay on the versions route");
    expectStory(Boolean(candidateAction && !candidateAction.disabled), "dockrev candidate card should stay enabled before navigation");
    expectStory(Boolean(topAction && !topAction.disabled), "top-level dockrev self-upgrade action should stay enabled before navigation");
    expectStory(Boolean(newerAction?.disabled), "newer non-candidate dockrev release should remain disabled in the visual state");
    expectStory(normalizeText(newerAction?.textContent).includes("仅候选可升级"), "newer non-candidate dockrev release should use the candidate-only disabled label in the visual state");
    expectStory(newerAction?.className.includes("btnGhost") ?? false, "newer non-candidate dockrev release should render with the muted disabled ghost affordance");
    expectStory(normalizeText(findVersionCard(canvasElement, "0.62.0")?.textContent).includes("当前候选 0.62.0 已就绪；点击后进入 Dockrev 自我升级流程。"), "candidate dockrev release should explain the active self-upgrade handoff in the visual state");
    expectStory(
      newerAsideWidth > 0 && newerAsideWidth < newerBodyWidth,
      "dockrev action rail should stay narrower than the main reading column",
    );
    expectStory(
      newerButtonWidth >= 140 && newerButtonWidth < newerBodyWidth,
      "dockrev self-upgrade button should stay narrower than the main reading column",
    );
  },
};

export const DockrevVersionsSelfUpgradeOffline: ServiceDetailStory = {
  parameters: {
    ...dockrevSelfUpgradeStoryParameters,
    dockrevSupervisorSelfUpgradeResponse: {
      status: 503,
      body: { message: "supervisor offline" },
    },
  },
  render: render("stack-prod", "svc-prod-api", "versions", "supervisor offline 时，Dockrev 版本卡与顶部入口都禁用，但重试只保留在顶部。"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findVersionCard(canvasElement, "0.62.0")));
    const doc = canvasElement.ownerDocument;
    const candidateAction = findVersionAction(canvasElement, "update", "0.62.0");
    const newerAction = findVersionAction(canvasElement, "update", "0.63.0");
    const topAction = findButtons(doc, "升级 Dockrev").find(
      (button) => !button.closest('[data-service-version-action="update"]'),
    );

    await waitForCondition(() => normalizeText(findVersionCard(canvasElement, "0.62.0")?.textContent).includes("自我升级不可用（supervisor offline）"));
    expectStory(Boolean(candidateAction?.disabled), "dockrev candidate card should disable itself when supervisor is offline");
    expectStory(Boolean(topAction?.disabled), "top-level dockrev self-upgrade action should also disable when supervisor is offline");
    expectStory(Boolean(findButton(doc, "重试")), "offline dockrev self-upgrade should keep the retry entry only in the top actions");
    expectStory(normalizeText(findVersionCard(canvasElement, "0.62.0")?.textContent).includes("自我升级不可用（supervisor offline）"), "candidate card should surface the offline reason");
    expectStory(Boolean(newerAction?.disabled), "newer non-candidate dockrev release should remain disabled while offline");
    expectStory(normalizeText(newerAction?.textContent).includes("仅候选可升级"), "offline non-candidate dockrev release should still keep the candidate-only label");
    expectStory(normalizeText(findVersionCard(canvasElement, "0.63.0")?.textContent).includes("自我升级不可用（supervisor offline）"), "non-candidate dockrev release should surface the real offline blocker");
    expectStory(normalizeText(findVersionCard(canvasElement, "0.63.0")?.textContent).includes("当前候选为 0.62.0，此版本不能直接升级。"), "offline non-candidate dockrev release should still explain why this specific card cannot upgrade");
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
  render: render("stack-prod", "svc-prod-api", "versions", undefined, {
    pageTitle: null,
    pageSubtitle: null,
  }),
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
