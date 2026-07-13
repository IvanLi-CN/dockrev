import type { JobListItem, ServiceBackupRecordsResponse, ServiceLogSnapshotResponse } from "../../api";

export function buildLongLogsSnapshot(serviceId: string, count = 1600): ServiceLogSnapshotResponse {
  const startedAt = Date.parse("2026-06-29T08:00:00.000Z");
  return {
    serviceId,
    lines: Array.from({ length: count }, (_, index) => {
      const ts = new Date(startedAt + index * 1_000).toISOString();
      const base =
        index % 7 === 0
          ? `GET /internal/metrics 200 trace=req-${String(index).padStart(4, "0")} cache=warm upstream=payments-v2 latency=${40 + (index % 11)}ms region=ap-southeast-1 release=2026.06.29-${(index % 5) + 1}`
          : `worker cycle=${index} queue=critical state=idle lease=svc-prod-api lock=refresh-${String(index).padStart(4, "0")}`;
      const raw = index % 11 === 0 ? `\u001b[33m${base}\u001b[0m` : index % 13 === 0 ? `\u001b[31m${base}\u001b[0m` : base;
      return { ts, raw, plain: raw };
    }),
    lastEventId: count,
    bufferLimit: 2000,
  };
}

export function buildMultilineLogsSnapshot(serviceId: string): ServiceLogSnapshotResponse {
  const multilineRaw = [
    "\u001b[2m2026-07-01T08:12:51.833063Z\u001b[0m \u001b[33m WARN\u001b[0m failed to broadcast pool attempt start runtime snapshot err=error returned from database: (code: 5) database is locked",
    "",
    "Caused by:",
    "    (code: 5) database is locked invoke_id=proxy-1281-1782893570550",
  ].join("\n");
  return {
    serviceId,
    lines: [
      {
        ts: "2026-07-01T08:12:51.833063000Z",
        raw: multilineRaw,
        plain: multilineRaw,
      },
      {
        ts: "2026-07-01T08:12:53.763043000Z",
        raw: "\u001b[2m2026-07-01T08:12:53.763043Z\u001b[0m \u001b[32m INFO\u001b[0m openai proxy response headers ready proxy_request_id=1279 method=POST uri=/v1/responses status=200 OK elapsed_ms=10542",
        plain: "\u001b[2m2026-07-01T08:12:53.763043Z\u001b[0m \u001b[32m INFO\u001b[0m openai proxy response headers ready proxy_request_id=1279 method=POST uri=/v1/responses status=200 OK elapsed_ms=10542",
      },
    ],
    lastEventId: 2,
    bufferLimit: 2000,
  };
}

export const paginatedHistoryJobs: JobListItem[] = Array.from({ length: 23 }, (_, index) => {
  const sequence = 23 - index;
  const timestamp = new Date(Date.parse("2026-07-12T16:30:00.000Z") - index * 60_000).toISOString();
  return {
    id: `job-history-page-${sequence}`,
    type: index % 5 === 0 ? "rollback" : "update",
    scope: "service",
    stackId: "stack-prod",
    serviceId: "svc-prod-api",
    status: index % 7 === 0 ? "rolled_back" : "success",
    createdBy: index % 2 === 0 ? "ivan" : "auto-policy",
    reason: index % 2 === 0 ? "ui" : "auto_policy",
    createdAt: timestamp,
    startedAt: timestamp,
    finishedAt: timestamp,
    allowArchMismatch: false,
    backupMode: "inherit",
    summary: { serviceId: "svc-prod-api" },
  };
});

export const historyReleaseNotes = Array.from({ length: 28 }, (_, index) => {
  const tagName = index === 22 ? "5.2.4" : `5.1.${28 - index}`;
  return {
    id: 70_000 + index,
    tagName,
    name: tagName,
    body: `Release ${tagName}\n\n- 修复部署流程中的边界问题。\n- 改进任务状态的可读性。`,
    htmlUrl: `https://github.com/acme/api/releases/tag/${tagName}`,
    draft: false,
    prerelease: false,
    publishedAt: new Date(Date.UTC(2026, 6, 12, 16, 30) - index * 3_600_000).toISOString(),
    createdAt: new Date(Date.UTC(2026, 6, 12, 16, 15) - index * 3_600_000).toISOString(),
  };
});

export const partialHistoryBackupRecords: ServiceBackupRecordsResponse = {
  records: [
    {
      backupId: "bkp-partial-size",
      jobId: "job-auto-policy-api-5-2-3",
      scope: "service",
      status: "success",
      createdAt: "2026-07-12T15:15:00.000Z",
      finishedAt: "2026-07-12T15:16:00.000Z",
      artifactPath: "/srv/dockrev/backups/stack-prod/20260712-151500.tar.gz",
      sizeBytes: 18_432_000,
      cleanupAfter: "2026-07-13T15:15:00.000Z",
      deletedAt: null,
      error: null,
      assets: [
        {
          target: { kind: "bind-mount", path: "/var/lib/api/data" },
          status: "included",
          policy: "live_backup",
          sizeBytes: 12_288_000,
          reason: null,
        },
        {
          target: { kind: "docker-volume", name: "api-cache" },
          status: "included",
          policy: "stop_related_services",
          sizeBytes: null,
          reason: null,
        },
      ],
    },
  ],
};
