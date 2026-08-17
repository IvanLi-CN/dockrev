import type { ArchMatch, AutoUpdatePolicy, Service, ServiceHomepage, ServiceResourceOverviewResponse } from "../api";
import type { RowStatus } from "../updateStatus";

export const HOMEPAGE_SNAPSHOT_KEY = "dockrev.homepage.snapshot.v2";
export const HOMEPAGE_NAV_SNAPSHOT_KEY = "dockrev.homepage.navSnapshot.v1";
export const HOMEPAGE_RESOURCE_SUMMARY_KEY =
  "dockrev.homepage.resourceSummary.v1";

const SNAPSHOT_VERSION = 2;

const ROW_STATUSES = new Set<RowStatus>([
  "ok",
  "updatable",
  "hint",
  "archMismatch",
  "blocked",
]);

export type HomepageSnapshotCard = {
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
  icon: string | null;
  status: RowStatus;
  isDockrev: boolean;
  service: Service;
};

export type HomepageSnapshotV2 = {
  version: 2;
  generatedAt: string;
  lastCheckAt: string | null;
  resourceSummary: ServiceResourceOverviewResponse;
  cards: HomepageSnapshotCard[];
};

type LegacyHomepageNavCardSnapshotItem = {
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
  icon: string | null;
  status: RowStatus;
  isDockrev: boolean;
};

type LegacyHomepageNavSnapshot = {
  version: 1;
  generatedAt: string;
  cards: LegacyHomepageNavCardSnapshotItem[];
};

type LegacyHomepageResourceSummarySnapshot = {
  version: 1;
  generatedAt: string;
  overview: ServiceResourceOverviewResponse;
};

type SnapshotStorage = Pick<Storage, "getItem" | "setItem" | "removeItem">;

function browserStorage(): SnapshotStorage | null {
  try {
    return typeof window === "undefined" ? null : window.localStorage;
  } catch {
    return null;
  }
}

function readJson(storage: SnapshotStorage | null, key: string): unknown {
  if (!storage) return null;
  try {
    const raw = storage.getItem(key);
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

function writeJson(storage: SnapshotStorage | null, key: string, value: unknown) {
  if (!storage) return;
  try {
    storage.setItem(key, JSON.stringify(value));
  } catch {
    // localStorage is best-effort only.
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function asString(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function asOptionalString(value: unknown): string | null {
  return value == null || typeof value === "string" ? (value ?? null) : null;
}

export function normalizeHomepageHref(
  value: string | null | undefined,
): string | null {
  const trimmed = (value ?? "").trim();
  if (
    !trimmed ||
    [...trimmed].some((char) => {
      const code = char.charCodeAt(0);
      return code <= 0x1f || code === 0x7f;
    }) ||
    trimmed.includes("\\")
  ) {
    return null;
  }
  if (trimmed.startsWith("/") && !trimmed.startsWith("//")) return trimmed;
  try {
    const url = new URL(trimmed);
    if (url.protocol === "http:" || url.protocol === "https:") {
      return url.toString();
    }
  } catch {
    return null;
  }
  return null;
}

export function canRestorePersistedHomepageSnapshot(
  status: "missing" | "fresh" | "stale" | "expired" | "unsupported",
): status is "fresh" | "stale" {
  return status === "fresh" || status === "stale";
}

function parseService(value: unknown): Service | null {
  if (!isRecord(value)) return null;
  const id = asString(value.id);
  const name = asString(value.name);
  if (!id || !name) return null;
  if (!isRecord(value.image)) return null;
  const imageRef = asString(value.image.ref);
  const imageTag = asString(value.image.tag);
  if (!imageRef || !imageTag) return null;
  const homepage = isRecord(value.homepage)
    ? ({
        group: asOptionalString(value.homepage.group),
        name: asOptionalString(value.homepage.name),
        icon: asOptionalString(value.homepage.icon),
        href: asOptionalString(value.homepage.href),
        description: asOptionalString(value.homepage.description),
      } satisfies ServiceHomepage)
    : null;
  const candidate =
    isRecord(value.candidate) &&
    asString(value.candidate.tag) &&
    asString(value.candidate.digest)
      ? {
          tag: value.candidate.tag as string,
          resolvedTag: asOptionalString(value.candidate.resolvedTag),
          digest: value.candidate.digest as string,
          archMatch:
            typeof value.candidate.archMatch === "string"
              ? (value.candidate.archMatch as ArchMatch)
              : "unknown",
          arch: Array.isArray(value.candidate.arch)
            ? value.candidate.arch.filter((item): item is string => typeof item === "string")
            : [],
        }
      : null;
  const ignore =
    isRecord(value.ignore) &&
    typeof value.ignore.matched === "boolean" &&
    asString(value.ignore.ruleId) &&
    asString(value.ignore.reason)
      ? {
          matched: value.ignore.matched as boolean,
          ruleId: value.ignore.ruleId as string,
          reason: value.ignore.reason as string,
        }
      : null;
  const versionInference =
    isRecord(value.versionInference) && asString(value.versionInference.status)
      ? {
          status: value.versionInference.status as string,
          reason: asOptionalString(value.versionInference.reason),
          checkedAt: asOptionalString(value.versionInference.checkedAt),
        }
      : null;
  const settings = isRecord(value.settings)
    ? {
        autoRollback: Boolean(value.settings.autoRollback),
        backupTargets: isRecord(value.settings.backupTargets)
          ? {
              bindPaths: isRecord(value.settings.backupTargets.bindPaths)
                ? (value.settings.backupTargets.bindPaths as Record<string, "inherit" | "skip" | "force">)
                : {},
              volumeNames: isRecord(value.settings.backupTargets.volumeNames)
                ? (value.settings.backupTargets.volumeNames as Record<string, "inherit" | "skip" | "force">)
                : {},
            }
          : { bindPaths: {}, volumeNames: {} },
        repoUrl: asOptionalString(value.settings.repoUrl),
        ...(isRecord(value.settings.autoUpdatePolicy)
          ? { autoUpdatePolicy: value.settings.autoUpdatePolicy as AutoUpdatePolicy }
          : {}),
      }
    : null;
  if (!settings) return null;
  return {
    id,
    name,
    image: {
      ref: imageRef,
      tag: imageTag,
      digest: asOptionalString(value.image.digest),
      resolvedTag: asOptionalString(value.image.resolvedTag),
      resolvedTags: Array.isArray(value.image.resolvedTags)
        ? value.image.resolvedTags.filter((item): item is string => typeof item === "string")
        : null,
    },
    homepage,
    candidate,
    ignore,
    versionInference,
    newVersionDiscoveryCount:
      typeof value.newVersionDiscoveryCount === "number"
        ? value.newVersionDiscoveryCount
        : null,
    settings,
    archived: typeof value.archived === "boolean" ? value.archived : undefined,
  };
}

function parseResourceSummary(value: unknown): ServiceResourceOverviewResponse | null {
  if (!isRecord(value)) return null;
  if (typeof value.enabled !== "boolean") return null;
  if (typeof value.generatedAt !== "string") return null;
  if (typeof value.staleAfterSeconds !== "number") return null;
  if (!Array.isArray(value.services)) return null;
  return value as ServiceResourceOverviewResponse;
}

function parseSnapshotCard(value: unknown): HomepageSnapshotCard | null {
  if (!isRecord(value)) return null;
  const status = asString(value.status);
  if (!status || !ROW_STATUSES.has(status as RowStatus)) return null;
  const required = {
    id: asString(value.id),
    stackId: asString(value.stackId),
    stackName: asString(value.stackName),
    serviceId: asString(value.serviceId),
    serviceName: asString(value.serviceName),
    imageRef: asString(value.imageRef),
    groupName: asString(value.groupName),
    title: asString(value.title),
    description: asString(value.description),
    href: asString(value.href),
  };
  if (Object.values(required).some((item) => item == null)) return null;
  const href = normalizeHomepageHref(required.href);
  if (!href) return null;
  if (typeof value.isDockrev !== "boolean") return null;
  const service = parseService(value.service);
  if (!service) return null;
  return {
    id: required.id ?? "",
    stackId: required.stackId ?? "",
    stackName: required.stackName ?? "",
    serviceId: required.serviceId ?? "",
    serviceName: required.serviceName ?? "",
    imageRef: required.imageRef ?? "",
    groupName: required.groupName ?? "",
    title: required.title ?? "",
    description: required.description ?? "",
    href: required.href ?? "",
    icon: asOptionalString(value.icon),
    status: status as RowStatus,
    isDockrev: value.isDockrev,
    service,
  };
}

function parseSnapshotV2(value: unknown): HomepageSnapshotV2 | null {
  if (!isRecord(value)) return null;
  if (value.version !== SNAPSHOT_VERSION) return null;
  const generatedAt = asString(value.generatedAt);
  if (!generatedAt || Number.isNaN(Date.parse(generatedAt))) return null;
  const resourceSummary = parseResourceSummary(value.resourceSummary);
  if (!resourceSummary) return null;
  if (!Array.isArray(value.cards)) return null;
  const cards = value.cards.map(parseSnapshotCard);
  if (cards.some((card) => card === null)) return null;
  const lastCheckAt =
    value.lastCheckAt == null ? null : asString(value.lastCheckAt);
  if (value.lastCheckAt != null && !lastCheckAt) return null;
  return {
    version: SNAPSHOT_VERSION,
    generatedAt,
    lastCheckAt,
    resourceSummary,
    cards: cards as HomepageSnapshotCard[],
  };
}

function parseLegacyNavSnapshot(value: unknown): LegacyHomepageNavSnapshot | null {
  if (!isRecord(value) || value.version !== 1) return null;
  if (!Array.isArray(value.cards)) return null;
  const generatedAt = asString(value.generatedAt);
  if (!generatedAt || Number.isNaN(Date.parse(generatedAt))) return null;
  const cards = value.cards.map((entry) => {
    if (!isRecord(entry)) return null;
    const status = asString(entry.status);
    if (!status || !ROW_STATUSES.has(status as RowStatus)) return null;
    const required = {
      id: asString(entry.id),
      stackId: asString(entry.stackId),
      stackName: asString(entry.stackName),
      serviceId: asString(entry.serviceId),
      serviceName: asString(entry.serviceName),
      imageRef: asString(entry.imageRef),
      groupName: asString(entry.groupName),
      title: asString(entry.title),
      description: asString(entry.description),
      href: asString(entry.href),
    };
    if (Object.values(required).some((item) => item == null)) return null;
    const href = normalizeHomepageHref(required.href);
    if (!href) return null;
    return {
      id: required.id ?? "",
      stackId: required.stackId ?? "",
      stackName: required.stackName ?? "",
      serviceId: required.serviceId ?? "",
      serviceName: required.serviceName ?? "",
      imageRef: required.imageRef ?? "",
      groupName: required.groupName ?? "",
      title: required.title ?? "",
      description: required.description ?? "",
      href: required.href ?? "",
      icon: asOptionalString(entry.icon),
      status: status as RowStatus,
      isDockrev: Boolean(entry.isDockrev),
    } satisfies LegacyHomepageNavCardSnapshotItem;
  });
  if (cards.some((card) => card === null)) return null;
  return { version: 1, generatedAt, cards: cards as LegacyHomepageNavCardSnapshotItem[] };
}

function parseLegacyResourceSnapshot(
  value: unknown,
): LegacyHomepageResourceSummarySnapshot | null {
  if (!isRecord(value) || value.version !== 1) return null;
  const generatedAt = asString(value.generatedAt);
  if (!generatedAt || Number.isNaN(Date.parse(generatedAt))) return null;
  const overview = parseResourceSummary(value.overview);
  if (!overview) return null;
  return { version: 1, generatedAt, overview };
}

function markServiceResourceOverviewStale(
  overview: ServiceResourceOverviewResponse,
): ServiceResourceOverviewResponse {
  return {
    ...overview,
    services: overview.services.map((item) => ({
      ...item,
      stale: item.sampledAt ? true : item.stale,
    })),
  };
}

function isResourceSummaryStale(
  generatedAt: string,
  summary: ServiceResourceOverviewResponse,
  nowMs = Date.now(),
): boolean {
  const generatedAtMs = Date.parse(generatedAt);
  if (!Number.isFinite(generatedAtMs)) return true;
  const staleAfterMs = Math.max(60, summary.staleAfterSeconds) * 1000;
  return nowMs - generatedAtMs > staleAfterMs;
}

export function homepageSnapshotIsFresh(
  snapshot: HomepageSnapshotV2,
  nowMs = Date.now(),
): boolean {
  return !isResourceSummaryStale(snapshot.generatedAt, snapshot.resourceSummary, nowMs);
}

function legacyCardToSnapshotCard(card: LegacyHomepageNavCardSnapshotItem): HomepageSnapshotCard {
  const legacyCandidateStatus = card.status === "updatable" ? "hint" : card.status;
  return {
    ...card,
    status: legacyCandidateStatus,
    service: {
      id: card.serviceId,
      name: card.serviceName,
      image: {
        ref: card.imageRef,
        tag: card.imageRef.split(":").pop() ?? "latest",
        digest: null,
        resolvedTag: null,
        resolvedTags: null,
      },
      homepage: {
        group: card.groupName,
        name: card.title,
        icon: card.icon,
        href: card.href,
        description: card.description,
      },
      candidate: null,
      ignore: card.status === "blocked"
        ? {
            matched: true,
            ruleId: "snapshot-legacy",
            reason: "legacy snapshot",
          }
        : null,
      versionInference: {
        status: "ready",
        reason: null,
        checkedAt: null,
      },
      newVersionDiscoveryCount: null,
      settings: {
        autoRollback: true,
        backupTargets: { bindPaths: {}, volumeNames: {} },
        repoUrl: null,
      },
      archived: false,
    },
  };
}

function tryMigrateLegacySnapshot(
  storage: SnapshotStorage | null,
): HomepageSnapshotV2 | null {
  const nav = parseLegacyNavSnapshot(readJson(storage, HOMEPAGE_NAV_SNAPSHOT_KEY));
  const resource = parseLegacyResourceSnapshot(
    readJson(storage, HOMEPAGE_RESOURCE_SUMMARY_KEY),
  );
  if (!nav) return null;
  const resourceSummary = resource
    ? isResourceSummaryStale(resource.generatedAt, resource.overview)
      ? markServiceResourceOverviewStale(resource.overview)
      : resource.overview
    : {
        enabled: false,
        window: "1h",
        generatedAt: nav.generatedAt,
        staleAfterSeconds: 60,
        services: [],
      };
  const snapshot: HomepageSnapshotV2 = {
    version: SNAPSHOT_VERSION,
    generatedAt: resource?.generatedAt ?? nav.generatedAt,
    lastCheckAt: null,
    resourceSummary,
    cards: nav.cards.map(legacyCardToSnapshotCard),
  };
  if (!homepageSnapshotIsFresh(snapshot)) return null;
  writeJson(storage, HOMEPAGE_SNAPSHOT_KEY, snapshot);
  try {
    storage?.removeItem(HOMEPAGE_NAV_SNAPSHOT_KEY);
    storage?.removeItem(HOMEPAGE_RESOURCE_SUMMARY_KEY);
  } catch {
    // Best effort cleanup only.
  }
  return snapshot;
}

export function readHomepageSnapshot(
  storage: SnapshotStorage | null = browserStorage(),
): HomepageSnapshotV2 | null {
  const current = parseSnapshotV2(readJson(storage, HOMEPAGE_SNAPSHOT_KEY));
  if (current && homepageSnapshotIsFresh(current)) return current;
  return tryMigrateLegacySnapshot(storage);
}

export function writeHomepageSnapshot(
  snapshot: HomepageSnapshotV2,
  storage: SnapshotStorage | null = browserStorage(),
) {
  writeJson(storage, HOMEPAGE_SNAPSHOT_KEY, snapshot);
}

export function removeHomepageSnapshots(
  storage: SnapshotStorage | null = browserStorage(),
) {
  if (!storage) return;
  try {
    storage.removeItem(HOMEPAGE_SNAPSHOT_KEY);
    storage.removeItem(HOMEPAGE_NAV_SNAPSHOT_KEY);
    storage.removeItem(HOMEPAGE_RESOURCE_SUMMARY_KEY);
  } catch {
    // Best effort cleanup for tests and corrupted browser storage.
  }
}

export function homepageSnapshotIsResourceStale(
  snapshot: HomepageSnapshotV2,
  nowMs = Date.now(),
): boolean {
  return isResourceSummaryStale(snapshot.generatedAt, snapshot.resourceSummary, nowMs);
}

export function markHomepageSnapshotResourceStale(
  snapshot: HomepageSnapshotV2,
): HomepageSnapshotV2 {
  return {
    ...snapshot,
    resourceSummary: markServiceResourceOverviewStale(snapshot.resourceSummary),
  };
}

export function homepageSnapshotFromResponse(
  input: {
    generatedAt: string;
    lastCheckAt?: string | null;
    resourceSummary: ServiceResourceOverviewResponse;
    cards: HomepageSnapshotCard[];
  },
): HomepageSnapshotV2 {
  return {
    version: SNAPSHOT_VERSION,
    generatedAt: input.generatedAt,
    lastCheckAt: input.lastCheckAt ?? null,
    resourceSummary: input.resourceSummary,
    cards: input.cards,
  };
}
