import { type ServiceReleaseNoteItem, type ServiceRollbackTargetResponse } from "../api";
import { cn } from "../lib/utils";
import { shortDigest } from "../pages/serviceDetailUtils";
import { Button, Mono, Pill } from "../ui";
import { formatReleaseDate, preferredReleaseTimestamp, safeHttpUrl } from "./serviceVersionsSectionUtils";

const BODY_COLLAPSE_LINE_COUNT = 10;

export type ServiceVersionCardModel = {
  item: ServiceReleaseNoteItem;
  body: string;
  bodyMissing: boolean;
  currentMatch: boolean;
  candidateMatch: boolean;
  deployedHistorical: boolean;
  rollbackTargetMatch: boolean;
  olderThanCurrent: boolean;
  showUpdate: boolean;
  showRollback: boolean;
  updateDisabledReason: string | null;
  rollbackDisabledReason: string | null;
};

function collapseBody(body: string, expanded: boolean): {
  visibleBody: string;
  totalLines: number;
  isCollapsible: boolean;
} {
  const trimmed = body.trim();
  if (!trimmed) {
    return { visibleBody: "", totalLines: 0, isCollapsible: false };
  }
  const lines = trimmed.split(/\r?\n/);
  if (expanded || lines.length <= BODY_COLLAPSE_LINE_COUNT) {
    return {
      visibleBody: trimmed,
      totalLines: lines.length,
      isCollapsible: lines.length > BODY_COLLAPSE_LINE_COUNT,
    };
  }
  return {
    visibleBody: lines.slice(0, BODY_COLLAPSE_LINE_COUNT).join("\n"),
    totalLines: lines.length,
    isCollapsible: true,
  };
}

export function ServiceVersionCard(props: {
  card: ServiceVersionCardModel;
  candidateDisplayVersion: string | null;
  rollbackTarget: ServiceRollbackTargetResponse | null;
  viewLabel: string;
  sourceLabel: string;
  expanded: boolean;
  onToggleExpanded: (id: string) => void;
  onApplyUpdate: () => void;
  onRollback: () => void;
  onOpenRollbackExplanation: (item: ServiceReleaseNoteItem) => void;
}) {
  const { card } = props;
  const linkUrl = safeHttpUrl(card.item.htmlUrl);
  const bodyState = collapseBody(card.body, props.expanded);
  const releaseDate = formatReleaseDate(preferredReleaseTimestamp(card.item));
  const canExecuteRollback =
    card.rollbackTargetMatch &&
    props.rollbackTarget?.available &&
    Boolean(props.rollbackTarget?.targetDigest);
  const showCandidateStatus = Boolean(card.candidateMatch && props.candidateDisplayVersion);
  const showRollbackDigestStatus = Boolean(
    card.rollbackTargetMatch && props.rollbackTarget?.targetDigest,
  );
  const rollbackTargetDigest = props.rollbackTarget?.targetDigest ?? null;
  const showCardAside =
    card.showUpdate ||
    card.showRollback ||
    showCandidateStatus ||
    showRollbackDigestStatus;
  const titleText = (card.item.name ?? "").trim() || card.item.tagName;
  const titleUsesTag = titleText === card.item.tagName;

  return (
    <article
      className={cn(
        "serviceVersionCard",
        card.olderThanCurrent && "serviceVersionCardOlder",
        card.currentMatch && "serviceVersionCardCurrent",
      )}
      data-service-version-card="true"
      data-release-tag={card.item.tagName}
      data-version-card-current={card.currentMatch ? "true" : "false"}
      data-version-card-older={card.olderThanCurrent ? "true" : "false"}
      data-version-card-has-actions={card.showUpdate || card.showRollback ? "true" : "false"}
      data-version-card-has-aside={showCardAside ? "true" : "false"}
    >
      <div className="serviceVersionCardMeta">
        <div className="serviceVersionHeading">
          <div className="serviceVersionTagRow">
            <div className="serviceVersionTagText">
              <Mono>{card.item.tagName}</Mono>
            </div>
            <div className="serviceVersionBadges">
              {card.currentMatch ? <Pill tone="ok">当前部署</Pill> : null}
              {card.candidateMatch ? <Pill tone="info">候选</Pill> : null}
              {card.deployedHistorical ? <Pill tone="muted">已部署历史</Pill> : null}
              {card.rollbackTargetMatch ? <Pill tone="warn">可执行回滚</Pill> : null}
              {card.item.prerelease ? <Pill tone="muted">预发布</Pill> : null}
            </div>
          </div>
        </div>

        <dl className="serviceVersionFacts">
          <div>
            <dt>发布时间</dt>
            <dd className="serviceVersionDateValue">
              <span>{releaseDate.dateLine}</span>
              {releaseDate.timeLine ? <span>{releaseDate.timeLine}</span> : null}
            </dd>
          </div>
          <div>
            <dt>来源</dt>
            <dd>{props.sourceLabel}</dd>
          </div>
          <div>
            <dt>视图</dt>
            <dd>{props.viewLabel}</dd>
          </div>
          <div>
            <dt>状态</dt>
            <dd>{card.olderThanCurrent ? "相对当前更旧" : card.currentMatch ? "当前部署中" : "发布记录"}</dd>
          </div>
        </dl>

        {linkUrl ? (
          <a
            className="serviceVersionLinkRow"
            href={linkUrl}
            rel="noreferrer"
            target="_blank"
          >
            Release
          </a>
        ) : null}
      </div>

      <div className="serviceVersionCardBody">
        <div className="serviceVersionBodyShell">
          {!titleUsesTag ? <div className="serviceVersionBodyTitle">{titleText}</div> : null}
          {bodyState.visibleBody ? (
            <div
              className="serviceVersionBody"
              data-service-version-body-expanded={props.expanded ? "true" : "false"}
            >
              {bodyState.visibleBody}
            </div>
          ) : (
            <div className="serviceVersionBodyEmpty">该版本没有可展示的正文。</div>
          )}
        </div>
        {card.bodyMissing || bodyState.isCollapsible ? (
          <div className="serviceVersionBodyFoot">
            {card.bodyMissing ? (
              <span className="serviceVersionBodyHint">当前视图缺少专用内容，已回退原文。</span>
            ) : null}
            {bodyState.isCollapsible ? (
              <button
                type="button"
                className="serviceVersionExpandButton"
                onClick={() => props.onToggleExpanded(card.item.id)}
              >
                {props.expanded ? "收起" : "展开"}
              </button>
            ) : null}
          </div>
        ) : null}
      </div>

      <div className="serviceVersionCardAside" data-service-version-card-aside="true">
        {showCardAside ? (
          <>
          <div className="serviceVersionStatusStack">
            {showCandidateStatus ? (
              <div className="serviceVersionStatusBlock">
                <div className="serviceVersionStatusLabel">当前候选</div>
                <div className="serviceVersionStatusValue">
                  <Mono>{props.candidateDisplayVersion}</Mono>
                </div>
              </div>
            ) : null}
            {showRollbackDigestStatus ? (
              <div className="serviceVersionStatusBlock">
                <div className="serviceVersionStatusLabel">回滚目标摘要</div>
                <div className="serviceVersionStatusValue">
                  <Mono>{shortDigest(rollbackTargetDigest!)}</Mono>
                </div>
              </div>
            ) : null}
          </div>

          {card.showUpdate || card.showRollback ? (
            <div className="serviceVersionActionStack">
              {card.showUpdate ? (
                <div
                  className="serviceVersionActionBlock"
                  data-service-version-action="update"
                  data-release-tag={card.item.tagName}
                >
                  <Button
                    variant="primary"
                    disabled={Boolean(card.updateDisabledReason)}
                    hint={card.updateDisabledReason ?? undefined}
                    onClick={props.onApplyUpdate}
                  >
                    更新
                  </Button>
                  {card.updateDisabledReason ? (
                    <div className="serviceVersionActionHint">{card.updateDisabledReason}</div>
                  ) : (
                    <div className="serviceVersionActionHint">发起当前 candidate 对应的服务更新任务。</div>
                  )}
                </div>
              ) : null}

              {card.showRollback ? (
                <div
                  className="serviceVersionActionBlock"
                  data-service-version-action="rollback"
                  data-release-tag={card.item.tagName}
                >
                  <Button
                    variant={canExecuteRollback ? "danger" : "ghost"}
                    disabled={Boolean(card.rollbackDisabledReason)}
                    hint={card.rollbackDisabledReason ?? undefined}
                    onClick={() => {
                      if (canExecuteRollback) {
                        props.onRollback();
                        return;
                      }
                      props.onOpenRollbackExplanation(card.item);
                    }}
                  >
                    回滚
                  </Button>
                  <div className="serviceVersionActionHint">
                    {card.rollbackDisabledReason
                      ? card.rollbackDisabledReason
                      : canExecuteRollback
                        ? "这个版本正对应后端当前可执行的 rollback target。"
                        : "会进入解释性提示，不会直接创建回滚任务。"}
                  </div>
                </div>
              ) : null}
            </div>
          ) : null}
          </>
        ) : (
          <div className="serviceVersionCardAsidePlaceholder" aria-hidden="true" />
        )}
      </div>
    </article>
  );
}
