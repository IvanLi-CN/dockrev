import type {
  ServiceResourceOverviewResponse,
} from "../api";
import type { RowStatus } from "../updateStatus";

export const HOMEPAGE_NAV_SNAPSHOT_KEY = "dockrev.homepage.navSnapshot.v1";
export const HOMEPAGE_RESOURCE_SUMMARY_KEY =
  "dockrev.homepage.resourceSummary.v1";

const SNAPSHOT_VERSION = 1;

const ROW_STATUSES = new Set<RowStatus>([
  "ok",
  "updatable",
  "hint",
  "archMismatch",
  "blocked",
]);

export type HomepageNavCardSnapshotItem = {
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

export type HomepageNavSnapshot = {
  version: 1;
  generatedAt: string;
  cards: HomepageNavCardSnapshotItem[];
};

export type HomepageResourceSummarySnapshot = {
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
    // localStorage is an optimization. Quota or privacy-mode failures should
    // never block navigation rendering.
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

function parseNavCard(value: unknown): HomepageNavCardSnapshotItem | null {
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
  if (typeof value.isDockrev !== "boolean") return null;
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
  };
}

function parseNavSnapshot(value: unknown): HomepageNavSnapshot | null {
  if (!isRecord(value)) return null;
  if (value.version !== SNAPSHOT_VERSION) return null;
  const generatedAt = asString(value.generatedAt);
  if (!generatedAt || Number.isNaN(Date.parse(generatedAt))) return null;
  if (!Array.isArray(value.cards)) return null;
  const cards = value.cards.map(parseNavCard);
  if (cards.some((card) => card === null)) return null;
  return {
    version: SNAPSHOT_VERSION,
    generatedAt,
    cards: cards as HomepageNavCardSnapshotItem[],
  };
}

function parseResourceSnapshot(
  value: unknown,
): HomepageResourceSummarySnapshot | null {
  if (!isRecord(value)) return null;
  if (value.version !== SNAPSHOT_VERSION) return null;
  const generatedAt = asString(value.generatedAt);
  if (!generatedAt || Number.isNaN(Date.parse(generatedAt))) return null;
  if (!isRecord(value.overview)) return null;
  const overview = value.overview as Partial<ServiceResourceOverviewResponse>;
  if (typeof overview.enabled !== "boolean") return null;
  if (typeof overview.generatedAt !== "string") return null;
  if (typeof overview.staleAfterSeconds !== "number") return null;
  if (!Array.isArray(overview.services)) return null;
  return {
    version: SNAPSHOT_VERSION,
    generatedAt,
    overview: overview as ServiceResourceOverviewResponse,
  };
}

export function readHomepageNavSnapshot(
  storage: SnapshotStorage | null = browserStorage(),
): HomepageNavSnapshot | null {
  return parseNavSnapshot(readJson(storage, HOMEPAGE_NAV_SNAPSHOT_KEY));
}

export function writeHomepageNavSnapshot(
  cards: HomepageNavCardSnapshotItem[],
  storage: SnapshotStorage | null = browserStorage(),
  generatedAt = new Date().toISOString(),
) {
  const snapshot: HomepageNavSnapshot = {
    version: SNAPSHOT_VERSION,
    generatedAt,
    cards,
  };
  writeJson(storage, HOMEPAGE_NAV_SNAPSHOT_KEY, snapshot);
}

export function readHomepageResourceSummarySnapshot(
  storage: SnapshotStorage | null = browserStorage(),
): HomepageResourceSummarySnapshot | null {
  return parseResourceSnapshot(readJson(storage, HOMEPAGE_RESOURCE_SUMMARY_KEY));
}

export function writeHomepageResourceSummarySnapshot(
  overview: ServiceResourceOverviewResponse,
  storage: SnapshotStorage | null = browserStorage(),
  generatedAt = new Date().toISOString(),
) {
  const snapshot: HomepageResourceSummarySnapshot = {
    version: SNAPSHOT_VERSION,
    generatedAt,
    overview,
  };
  writeJson(storage, HOMEPAGE_RESOURCE_SUMMARY_KEY, snapshot);
}

export function removeHomepageSnapshots(
  storage: SnapshotStorage | null = browserStorage(),
) {
  if (!storage) return;
  try {
    storage.removeItem(HOMEPAGE_NAV_SNAPSHOT_KEY);
    storage.removeItem(HOMEPAGE_RESOURCE_SUMMARY_KEY);
  } catch {
    // Best effort cleanup for tests and corrupted browser storage.
  }
}

export function resourceSummarySnapshotIsStale(
  snapshot: HomepageResourceSummarySnapshot,
  nowMs = Date.now(),
): boolean {
  const generatedAtMs = Date.parse(snapshot.generatedAt);
  if (!Number.isFinite(generatedAtMs)) return true;
  const staleAfterMs = Math.max(60, snapshot.overview.staleAfterSeconds) * 1000;
  return nowMs - generatedAtMs > staleAfterMs;
}

export function markResourceOverviewStale(
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
