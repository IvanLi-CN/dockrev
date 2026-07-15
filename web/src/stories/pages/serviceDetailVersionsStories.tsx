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
import { expectStory, findButton, normalizeText, waitForCondition } from "./storyAssertions";

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
  render: render("stack-prod", "svc-prod-api", "versions", "版本子页以内联卡片展示 release notes，并以当前部署版本为锚点定位。"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "versions")));
    await waitForCondition(() => Boolean(findVersionCard(canvasElement, "5.2.1")));
    const viewport = canvasElement.querySelector<HTMLElement>(".serviceVersionsScrollViewport");
    const currentCard = findVersionCard(canvasElement, "5.2.1");
    const actionCard = findVersionCard(canvasElement, "5.2.2");
    const updateCandidate = findVersionAction(canvasElement, "update", "5.2.3");
    const updateDisabled = findVersionAction(canvasElement, "update", "5.4.4");
    const rollbackTarget = findVersionAction(canvasElement, "rollback", "5.2.0");
    const rollbackHint = findVersionAction(canvasElement, "rollback", "5.2.2");
    const totalCount = Number(
      canvasElement.querySelector('[data-service-versions="true"]')?.getAttribute(
        "data-service-versions-total-count",
      ) ?? "0",
    );
    const visibleCount = Number(
      canvasElement.querySelector('[data-service-versions="true"]')?.getAttribute(
        "data-service-versions-visible-count",
      ) ?? "0",
    );

    expectStory(currentRoutePathname() === "/services/stack-prod/svc-prod-api/versions", "versions deep link missing");
    expectStory(findTab(canvasElement, "versions")?.getAttribute("data-state") === "active", "versions tab should be active");
    expectStory(Boolean(viewport), "versions viewport missing");
    expectStory(Boolean(currentCard), "current version card missing");
    expectStory(
      getComputedStyle(currentCard ?? canvasElement).gridTemplateColumns.split(" ").filter(Boolean).length === 2,
      "read-only current version cards should collapse to a two-column layout instead of keeping an empty action rail",
    );
    const currentTitle = currentCard?.querySelector<HTMLElement>(".serviceVersionBodyTitle");
    const currentTag = currentCard?.querySelector<HTMLElement>(".serviceVersionTagText .mono");
    const currentMetaValue = currentCard?.querySelector<HTMLElement>(".serviceVersionFacts dd");
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
      !currentCard?.querySelector(".serviceVersionCardHeader"),
      "release title should not be hoisted into a cross-card header rail",
    );
    expectStory(
      !currentCard?.querySelector(".serviceVersionCardAside"),
      "read-only current version cards should not reserve a dedicated aside rail",
    );
    expectStory(
      normalizeText(currentCard?.textContent).includes("Release") &&
        !normalizeText(currentCard?.textContent).includes("发布页") &&
        !normalizeText(currentCard?.textContent).includes("查看 GitHub Release"),
      "GitHub release entry should collapse to a single direct Release link instead of label-plus-instruction copy",
    );
    expectStory(
      !(currentCard?.querySelector(".serviceVersionCardAside")?.textContent ?? "").includes("5.2.1"),
      "current version cards should not repeat the same deployed version summary inside the read-only aside",
    );
    expectStory(
      getComputedStyle(actionCard ?? canvasElement).gridTemplateColumns.split(" ").filter(Boolean).length === 3,
      "actionable desktop version cards should keep a dedicated third rail for status and actions",
    );
    expectStory(totalCount > 20 && visibleCount > 0 && visibleCount < totalCount, "versions list should stay virtualized");
    expectStory(updateCandidate && !updateCandidate.disabled, "candidate version should expose an enabled update action");
    expectStory(Boolean(updateDisabled?.disabled), "newer non-candidate release should render a disabled update action");
    expectStory(Boolean(rollbackTarget && !rollbackTarget.disabled), "rollback target version should expose an enabled rollback action");
    expectStory(Boolean(rollbackHint && !rollbackHint.disabled), "historically deployed old versions should keep an explanatory rollback entry");
    expectStory(findVersionCard(canvasElement, "5.2.0")?.getAttribute("data-version-card-older") === "true", "older comparable releases should render the greyscale state");
    expectStory(
      Math.abs(
        ((currentCard?.getBoundingClientRect().top ?? 0) + (currentCard?.getBoundingClientRect().height ?? 0) / 2) -
          ((viewport?.getBoundingClientRect().top ?? 0) + (viewport?.getBoundingClientRect().height ?? 0) / 2),
      ) < 220,
      "current deployed version should be centered inside the versions viewport on first render",
    );

    findButton(canvasElement, "原文")?.click();
    await waitForCondition(
      () =>
        canvasElement.querySelector('[data-service-versions="true"]')?.getAttribute("data-service-versions-view") ===
        "original",
    );
    const expandButton = Array.from(currentCard?.querySelectorAll<HTMLButtonElement>("button") ?? []).find(
      (button) => normalizeText(button.textContent) === "展开",
    );
    expectStory(expandButton, "long release notes should default to the collapsed state");
    expectStory(!normalizeText(currentCard?.textContent).includes("故意超过十行"), "collapsed release notes should hide lines beyond the first ten");
    expandButton.click();
    await waitForCondition(() => normalizeText(currentCard?.textContent).includes("故意超过十行"));

    rollbackHint?.click();
    const doc = canvasElement.ownerDocument;
    await waitForCondition(() => doc.body.textContent?.includes("不会直接创建回滚任务") ?? false);
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
  render: render("stack-prod", "svc-prod-api", "versions", "移动端版本卡收紧元信息栅格，不把右侧空间白白空出来。"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findVersionCard(canvasElement, "5.2.1")));
    const card = findVersionCard(canvasElement, "5.2.1");
    const factsGrid = card?.querySelector<HTMLElement>(".serviceVersionFacts");
    await waitForCondition(() => visibleVersionCards(canvasElement).length >= 2);
    const [firstVisibleCard, secondVisibleCard] = visibleVersionCards(canvasElement).sort(
      (left, right) => left.getBoundingClientRect().top - right.getBoundingClientRect().top,
    );
    const gridColumns = getComputedStyle(card ?? canvasElement).gridTemplateColumns.split(" ").filter(Boolean);
    const factsColumns = getComputedStyle(factsGrid ?? canvasElement).gridTemplateColumns.split(" ").filter(Boolean);
    const primaryButton = findVersionAction(canvasElement, "update", "5.2.3");

    expectStory(findTab(canvasElement, "versions")?.getAttribute("data-state") === "active", "mobile versions tab should stay active");
    expectStory(gridColumns.length === 1, "mobile versions cards should collapse into a single-column layout");
    expectStory(factsColumns.length === 2, "mobile versions metadata should stay in a compact two-column facts grid");
    expectStory(Boolean(primaryButton), "mobile versions card should keep the action region");
    expectStory(card?.scrollWidth != null && card.clientWidth > 0 && card.scrollWidth <= card.clientWidth + 1, "mobile versions cards should not overflow horizontally");
    expectStory((primaryButton?.getBoundingClientRect().width ?? 0) > 160, "mobile versions actions should expand to a readable tap target width");
    expectStory(
      (secondVisibleCard?.getBoundingClientRect().top ?? 0) >= (firstVisibleCard?.getBoundingClientRect().bottom ?? 0) - 1,
      "mobile versions virtual rows should not overlap after dynamic height measurement",
    );
  },
};
