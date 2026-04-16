import type { MouseEvent } from "react";
import type { Service } from "../api";
import type { AggregateUpdatePreviewListItem } from "../components/AggregateUpdatePreviewList";
import { isStrictSemverTag } from "../versionDisplay";
import type { RowStatus } from "../updateStatus";
import { isDockrevImageRef } from "../runtimeConfig";

export function formatShort(ts?: string | null) {
  if (!ts) return "-";
  const d = new Date(ts);
  if (Number.isNaN(d.valueOf())) return ts;
  return d.toLocaleString();
}

export function formatCompactDateTime(ts?: string | null) {
  if (!ts) return "-";
  const d = new Date(ts);
  if (Number.isNaN(d.valueOf())) return ts;
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  const h = String(d.getHours()).padStart(2, "0");
  const min = String(d.getMinutes()).padStart(2, "0");
  return `${m}/${day} ${h}:${min}`;
}

export function isDockrevService(svc: Service): boolean {
  return isDockrevImageRef(svc.image.ref);
}

export function shouldPrefetchFloatingCandidate(
  candidateTag: string | null | undefined,
  candidateResolvedTag: string | null | undefined,
  candidateDigest: string | null | undefined,
): boolean {
  const raw = (candidateTag ?? "").trim();
  if (raw === "-") return false;
  if (!raw || isStrictSemverTag(raw)) return false;
  if (isStrictSemverTag(candidateResolvedTag)) return false;
  return (candidateDigest ?? "").trim().length > 0;
}

export function matchesCandidateSearch(
  stackName: string,
  svc: Service,
  query: string,
): boolean {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return true;
  const haystack = [
    stackName,
    svc.name,
    svc.image.ref,
    svc.homepage?.name ?? "",
    svc.homepage?.description ?? "",
  ]
    .join(" ")
    .toLowerCase();
  return haystack.includes(normalized);
}

export function StackIcon(props: { variant: "collapsed" | "expanded" }) {
  return (
    <svg
      className="stackIcon"
      viewBox="0 0 24 24"
      aria-hidden="true"
      focusable="false"
    >
      {props.variant === "expanded" ? (
        <path d="m5 19l2.757-7.351A1 1 0 0 1 8.693 11H21a1 1 0 0 1 .986 1.164l-.996 5.211A2 2 0 0 1 19.026 19za2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h4l3 3h7a2 2 0 0 1 2 2v2" />
      ) : (
        <path d="M5 4h4l3 3h7a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2" />
      )}
    </svg>
  );
}

export function formatGroupSummary(
  services: number,
  counts: Record<Exclude<RowStatus, "ok">, number>,
) {
  const parts: string[] = [`${services} services`];
  if (counts.updatable > 0) parts.push(`${counts.updatable} 可更新`);
  if (counts.hint > 0) parts.push(`${counts.hint} 需确认`);
  if (counts.archMismatch > 0) parts.push(`${counts.archMismatch} 架构不匹配`);
  if (counts.blocked > 0) parts.push(`${counts.blocked} 被阻止`);
  return parts.join(" · ");
}

export function withAggregateDisplayName(
  items: Array<
    Pick<AggregateUpdatePreviewListItem, "svc" | "status" | "guardedDockrev">
  >,
  stackName?: string,
  stackId?: string,
): AggregateUpdatePreviewListItem[] {
  return items.map((item) => ({
    ...item,
    displayName: stackName ? `${stackName}/${item.svc.name}` : item.svc.name,
    stackId,
  }));
}

export function GroupGuide() {
  return <div className="groupGuide" aria-hidden="true" />;
}

export function stopRowLink(event: MouseEvent<HTMLAnchorElement>) {
  event.stopPropagation();
}
