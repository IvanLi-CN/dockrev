import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import {
  ApiError,
  getJob,
  getStack,
  listStacks,
  triggerCheck,
  type StackDetail,
  type StackListItem,
} from "../api";
import { HomepageServiceIcon } from "../components/HomepageServiceIcon";
import { currentHref, navigate } from "../routes";
import { Button, DiscoveryCountPill, Input, Mono, Pill } from "../ui";
import {
  noteFor,
  serviceRowStatus,
  statusLabel,
  type RowStatus,
} from "../updateStatus";
import { usePageResumeRefresh } from "../usePageResumeRefresh";

type HomepageNavCard = {
  id: string;
  stackId: string;
  stackName: string;
  serviceId: string;
  serviceName: string;
  imageRef: string;
  groupName: string;
  title: string;
  description: string;
  href: string;
  external: boolean;
  icon: string | null;
  status: RowStatus;
  note: string;
  discoveryCount: number;
};

function statusTone(
  status: RowStatus,
): "ok" | "warn" | "bad" | "muted" | "info" {
  if (status === "updatable") return "ok";
  if (status === "hint") return "warn";
  if (status === "archMismatch" || status === "blocked") return "bad";
  return "muted";
}

function summaryCounts(cards: HomepageNavCard[]) {
  return cards.reduce(
    (acc, card) => {
      acc.total += 1;
      if (card.status === "updatable") acc.updatable += 1;
      if (card.status === "hint") acc.hint += 1;
      if (card.status === "archMismatch") acc.archMismatch += 1;
      if (card.status === "blocked") acc.blocked += 1;
      return acc;
    },
    { total: 0, updatable: 0, hint: 0, archMismatch: 0, blocked: 0 },
  );
}

function normalizeHomepageHref(
  value: string | null | undefined,
): string | null {
  const trimmed = (value ?? "").trim();
  if (!trimmed) return null;
  if (trimmed.startsWith("/") && !trimmed.startsWith("//")) return trimmed;
  try {
    const url = new URL(trimmed);
    if (url.protocol === "http:" || url.protocol === "https:")
      return url.toString();
  } catch {
    return null;
  }
  return null;
}

function toNavCards(
  stacks: StackListItem[],
  details: Record<string, StackDetail | undefined>,
): HomepageNavCard[] {
  const cards: HomepageNavCard[] = [];
  for (const stack of stacks) {
    const detail = details[stack.id];
    if (!detail) continue;
    for (const service of detail.services) {
      if (service.archived) continue;
      const groupName = service.homepage?.group?.trim() || detail.name;
      const title = service.homepage?.name?.trim() || service.name;
      const description =
        service.homepage?.description?.trim() || service.image.ref;
      const homepageHref = normalizeHomepageHref(service.homepage?.href);
      const href =
        homepageHref ||
        currentHref({
          name: "service",
          stackId: stack.id,
          serviceId: service.id,
        });
      const status = serviceRowStatus(service);
      cards.push({
        id: service.id,
        stackId: stack.id,
        stackName: detail.name,
        serviceId: service.id,
        serviceName: service.name,
        imageRef: service.image.ref,
        groupName,
        title,
        description,
        href,
        external: Boolean(homepageHref),
        icon: service.homepage?.icon ?? null,
        status,
        note: noteFor(service, status),
        discoveryCount: service.newVersionDiscoveryCount ?? 0,
      });
    }
  }
  return cards;
}

function matchesSearch(card: HomepageNavCard, query: string): boolean {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return true;
  const haystack = [
    card.groupName,
    card.title,
    card.description,
    card.imageRef,
    card.stackName,
    card.serviceName,
  ]
    .join(" ")
    .toLowerCase();
  return haystack.includes(normalized);
}

export function OverviewPage(props: {
  onLastScanHint: (lastScan?: string) => void;
  onTopActions: (node: ReactNode) => void;
}) {
  const { onLastScanHint, onTopActions } = props;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [noticeCheckJobId, setNoticeCheckJobId] = useState<string | null>(null);
  const [stacks, setStacks] = useState<StackListItem[]>([]);
  const [details, setDetails] = useState<
    Record<string, StackDetail | undefined>
  >({});
  const [search, setSearch] = useState("");

  const refresh = useCallback(async () => {
    const nextStacks = await listStacks();
    const maxLastScan = nextStacks
      .map((item) => item.lastCheckAt)
      .sort()
      .at(-1);
    onLastScanHint(maxLastScan);
    setStacks(nextStacks);
    const nextDetails = await Promise.all(
      nextStacks.map(async (stack) => {
        try {
          return [stack.id, await getStack(stack.id)] as const;
        } catch {
          return [stack.id, undefined] as const;
        }
      }),
    );
    setDetails(Object.fromEntries(nextDetails));
  }, [onLastScanHint]);

  const requestRefresh = usePageResumeRefresh(refresh, {
    onError: (value: unknown) =>
      setError(value instanceof Error ? value.message : String(value)),
  });

  useEffect(() => {
    void requestRefresh().catch((value: unknown) =>
      setError(value instanceof Error ? value.message : String(value)),
    );
  }, [requestRefresh]);

  useEffect(() => {
    onTopActions(
      <>
        <Button
          variant="ghost"
          disabled={busy}
          onClick={() => {
            void (async () => {
              setBusy(true);
              setError(null);
              try {
                await requestRefresh();
              } catch (value: unknown) {
                setError(
                  value instanceof Error ? value.message : String(value),
                );
              } finally {
                setBusy(false);
              }
            })();
          }}
        >
          刷新
        </Button>
        <Button
          variant="ghost"
          disabled={busy}
          onClick={() => navigate({ name: "services" })}
        >
          运维大盘
        </Button>
        <Button
          variant="primary"
          disabled={busy}
          onClick={() => {
            void (async () => {
              setBusy(true);
              setError(null);
              setNoticeCheckJobId(null);
              try {
                const response = await triggerCheck("all");
                setNoticeCheckJobId(response.checkId);
                await requestRefresh();
              } catch (value: unknown) {
                if (value instanceof ApiError && value.status === 409) {
                  const details = value.details;
                  const existingJobId =
                    details &&
                    typeof details === "object" &&
                    details !== null &&
                    "existingJobId" in details &&
                    typeof (details as Record<string, unknown>)
                      .existingJobId === "string"
                      ? ((details as Record<string, unknown>)
                          .existingJobId as string)
                      : null;
                  if (existingJobId) setNoticeCheckJobId(existingJobId);
                  else setError(value.message);
                } else {
                  setError(
                    value instanceof Error ? value.message : String(value),
                  );
                }
              } finally {
                setBusy(false);
              }
            })();
          }}
        >
          立即扫描
        </Button>
      </>,
    );
  }, [busy, onTopActions, requestRefresh]);

  useEffect(() => {
    if (!noticeCheckJobId) return;
    let closed = false;
    let timer: number | null = null;

    const poll = async () => {
      try {
        const job = await getJob(noticeCheckJobId);
        if (closed) return;
        if (job.status === "queued" || job.status === "running") {
          timer = window.setTimeout(() => {
            void poll();
          }, 1200);
          return;
        }
      } catch {
        if (closed) return;
      }

      try {
        await requestRefresh();
      } catch (value: unknown) {
        if (!closed)
          setError(value instanceof Error ? value.message : String(value));
      }
    };

    timer = window.setTimeout(() => {
      void poll();
    }, 1200);

    return () => {
      closed = true;
      if (timer != null) window.clearTimeout(timer);
    };
  }, [noticeCheckJobId, requestRefresh]);

  const allCards = useMemo(
    () => toNavCards(stacks, details),
    [details, stacks],
  );
  const filteredCards = useMemo(
    () => allCards.filter((card) => matchesSearch(card, search)),
    [allCards, search],
  );
  const groupedCards = useMemo(() => {
    const groups = new Map<string, HomepageNavCard[]>();
    for (const card of filteredCards) {
      const current = groups.get(card.groupName) ?? [];
      current.push(card);
      groups.set(card.groupName, current);
    }
    return Array.from(groups.entries())
      .map(([groupName, cards]) => ({
        groupName,
        cards: [...cards].sort((left, right) =>
          left.title.localeCompare(right.title),
        ),
      }))
      .sort((left, right) => left.groupName.localeCompare(right.groupName));
  }, [filteredCards]);

  const stats = useMemo(() => summaryCounts(filteredCards), [filteredCards]);

  return (
    <div className="page">
      <div className="card homepageOverviewHero">
        <div className="sectionRow">
          <div>
            <div className="title">服务导航</div>
            <div className="muted">
              兼容 Homepage 基础标签，按分组快速打开你的自托管服务入口。
            </div>
          </div>
          <div
            style={{
              marginLeft: "auto",
              display: "flex",
              gap: 10,
              alignItems: "center",
            }}
          >
            <Input
              className="input"
              onChange={(event) => setSearch(event.target.value)}
              placeholder="搜索分组 / 服务 / 描述 / 镜像"
              value={search}
            />
            <div className="muted">
              {filteredCards.length}/{allCards.length}
            </div>
          </div>
        </div>
        <div className="chipRow" style={{ marginTop: 14 }}>
          <div className="chipStatic">分组 {groupedCards.length}</div>
          <div className="chipStatic">服务 {stats.total}</div>
          {stats.updatable > 0 ? (
            <div className="chipStatic">新版本 {stats.updatable}</div>
          ) : null}
          {stats.hint > 0 ? (
            <div className="chipStatic">需确认 {stats.hint}</div>
          ) : null}
          {stats.archMismatch > 0 ? (
            <div className="chipStatic">架构不匹配 {stats.archMismatch}</div>
          ) : null}
          {stats.blocked > 0 ? (
            <div className="chipStatic">被阻止 {stats.blocked}</div>
          ) : null}
        </div>
      </div>

      {groupedCards.length === 0 ? (
        <div className="card">
          <div className="muted">当前搜索条件下没有可展示的服务入口。</div>
        </div>
      ) : (
        groupedCards.map((group) => (
          <section key={group.groupName} className="homepageGroupSection">
            <div className="sectionRow homepageGroupHeader">
              <div>
                <div className="title">{group.groupName}</div>
                <div className="muted">{group.cards.length} 个入口</div>
              </div>
            </div>
            <div className="homepageCardGrid">
              {group.cards.map((card) => (
                <a
                  key={card.id}
                  className="homepageServiceCard"
                  href={card.href}
                  rel="noopener noreferrer"
                  target="_blank"
                >
                  <div className="homepageServiceCardTop">
                    <HomepageServiceIcon icon={card.icon} title={card.title} />
                    <div className="homepageServiceCardIdentity">
                      <div className="homepageServiceCardTitleRow">
                        <div className="homepageServiceCardTitle">
                          {card.title}
                        </div>
                        <Pill tone="muted">
                          {card.external ? "新窗口" : "详情"}
                        </Pill>
                      </div>
                      <div className="muted homepageServiceCardDescription">
                        {card.description}
                      </div>
                    </div>
                  </div>

                  <div className="homepageServiceCardMeta">
                    <span className="muted">
                      stack <Mono>{card.stackName}</Mono>
                    </span>
                    <span className="muted">
                      service <Mono>{card.serviceName}</Mono>
                    </span>
                  </div>

                  <div className="homepageServiceCardBadges">
                    {card.status !== "ok" ? (
                      <Pill tone={statusTone(card.status)}>
                        {statusLabel(card.status)}
                      </Pill>
                    ) : null}
                    <DiscoveryCountPill count={card.discoveryCount} />
                  </div>

                  {card.note ? (
                    <div className="muted homepageServiceCardNote">
                      {card.note}
                    </div>
                  ) : null}
                </a>
              ))}
            </div>
          </section>
        ))
      )}

      {error ? <div className="error">{error}</div> : null}
      {noticeCheckJobId ? (
        <div className="success">
          扫描任务 <Mono>{noticeCheckJobId}</Mono> ·{" "}
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => navigate({ name: "job", jobId: noticeCheckJobId })}
          >
            查看任务
          </Button>
        </div>
      ) : null}
      {busy ? <div className="muted">处理中…</div> : null}
    </div>
  );
}
