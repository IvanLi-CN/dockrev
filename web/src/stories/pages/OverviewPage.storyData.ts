import type { HomepageSnapshotV2 } from "../../pages/homepageSnapshot";
import type { StackDetail } from "../../api";

type ServiceOverride = Partial<StackDetail["services"][number]>;

export function defaultHomepageOverrides(): Record<string, ServiceOverride> {
  return {
    "svc-prod-api": {
      homepage: {
        group: "Brain",
        name: "Acme API",
        icon: "si-github",
        href: "https://api.example.com",
        description: "API gateway & auth",
      },
    },
    "svc-prod-web": {
      homepage: {
        group: "Brain",
        name: "Web Console",
        icon: "mdi-monitor-dashboard",
        href: "https://web.example.com",
        description: "Primary admin console",
      },
    },
    "svc-prod-worker": {
      homepage: {
        group: "Tools",
        name: "Background Jobs",
        icon: "mdi-cog-refresh-outline",
        href: null,
        description: "Queue workers & cron",
      },
    },
    "svc-infra-loki": {
      homepage: {
        group: "Media",
        name: "Loki",
        icon: "mdi-file-document-multiple-outline",
        href: "https://logs.example.com",
        description: "Log aggregation",
      },
    },
    "svc-infra-prom": {
      homepage: {
        group: "Tools",
        name: "Prometheus",
        icon: "prometheus.svg",
        href: "https://metrics.example.com",
        description: "Metrics & alerts",
      },
    },
    "svc-infra-postgres": {
      homepage: {
        group: "Infra",
        name: "Postgres",
        icon:
          "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/postgres.svg",
        href: "https://db.example.com",
        description: "Transactional database",
      },
    },
  };
}

export function denseHomepageOverrides(): Record<string, ServiceOverride> {
  return {
    "svc-prod-api": {
      homepage: {
        group: "Brain",
        name: "Acme API",
        icon: "si-github",
        href: "https://api.example.com",
        description: "API gateway & auth",
      },
    },
    "svc-prod-web": {
      homepage: {
        group: "Brain",
        name: "Web Console",
        icon: "mdi-monitor-dashboard",
        href: "https://web.example.com",
        description: "Primary admin console",
      },
    },
    "svc-prod-worker": {
      homepage: {
        group: "Ops",
        name: "Background Jobs",
        icon: "mdi-cog-refresh-outline",
        href: null,
        description: "Queue workers & cron",
      },
    },
    "svc-infra-loki": {
      homepage: {
        group: "Media",
        name: "Loki",
        icon: "mdi-file-document-multiple-outline",
        href: "https://logs.example.com",
        description: "Log aggregation",
      },
    },
    "svc-infra-prom": {
      homepage: {
        group: "Tools",
        name: "Prometheus",
        icon: "prometheus.svg",
        href: "https://metrics.example.com",
        description: "Metrics & alerts",
      },
    },
    "svc-infra-postgres": {
      homepage: {
        group: "Data",
        name: "Postgres",
        icon: "postgres.svg",
        href: "https://db.example.com",
        description: "Transactional database",
      },
    },
  };
}

export function auditProofHomepageOverrides(): Record<string, ServiceOverride> {
  return {
    "svc-prod-api": {
      homepage: {
        group: "Brain",
        name: "Acme API",
        icon: "si-github",
        href: "https://api.example.com",
        description: "API gateway & auth",
      },
    },
    "svc-prod-web": {
      homepage: {
        group: "Brain",
        name: "Web Console",
        icon: "mdi-monitor-dashboard",
        href: "https://web.example.com",
        description: "Primary admin console",
      },
    },
    "svc-prod-worker": {
      homepage: {
        group: "Ops",
        name: "Background Jobs",
        icon: "sh-home-assistant.png",
        href: null,
        description: "Queue workers & cron",
      },
    },
    "svc-infra-loki": {
      homepage: {
        group: "Media",
        name: "Loki",
        icon: "nested/unsafe.svg",
        href: "https://logs.example.com",
        description: "Log aggregation",
      },
    },
    "svc-infra-prom": {
      homepage: {
        group: "Tools",
        name: "Prometheus",
        icon: "prometheus.svg",
        href: "https://metrics.example.com",
        description: "Metrics & alerts",
      },
    },
    "svc-infra-postgres": {
      homepage: {
        group: "Data",
        name: "Postgres",
        icon:
          "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/postgres.svg",
        href: "https://db.example.com",
        description: "Transactional database",
      },
    },
  };
}

export function cachedHomepageSnapshot(
  generatedAt = new Date().toISOString(),
): HomepageSnapshotV2 {
  return {
    version: 2,
    generatedAt,
    lastCheckAt: "2026-01-18T06:10:00.000Z",
    resourceSummary: {
      enabled: true,
      window: "1h",
      generatedAt,
      staleAfterSeconds: 60,
      services: [
        {
          serviceId: "svc-prod-api",
          sampledAt: generatedAt,
          cpuPercent: 42,
          memUsedBytes: 512 * 1024 * 1024,
          memLimitBytes: 1024 * 1024 * 1024,
          netRxRateBps: 2048,
          netTxRateBps: 4096,
          stale: false,
          sampleCount: 12,
        },
      ],
    },
    cards: [
      {
        id: "cached-acme-api",
        stackId: "stack-prod",
        stackName: "prod",
        serviceId: "svc-prod-api",
        serviceName: "api",
        imageRef: "ghcr.io/acme/api:5.2.1",
        groupName: "Cached Brain",
        title: "Cached Acme API",
        description: "Cached API gateway",
        href: "https://cached-api.example.com",
        icon: "si-github",
        status: "updatable",
        isDockrev: false,
        service: {
          id: "svc-prod-api",
          name: "api",
          image: {
            ref: "ghcr.io/acme/api:5.2.1",
            tag: "5.2.1",
            digest: "sha256:cached-api",
            resolvedTag: "5.2.1",
            resolvedTags: ["5.2.1"],
          },
          homepage: {
            group: "Cached Brain",
            name: "Cached Acme API",
            icon: "si-github",
            href: "https://cached-api.example.com",
            description: "Cached API gateway",
          },
          candidate: {
            tag: "5.2.3",
            resolvedTag: "5.2.3",
            digest: "sha256:cached-candidate",
            archMatch: "match",
            arch: ["linux/amd64"],
          },
          ignore: null,
          versionInference: { status: "ready", reason: null, checkedAt: null },
          newVersionDiscoveryCount: 1,
          settings: {
            autoRollback: true,
            backupTargets: { bindPaths: {}, volumeNames: {} },
            repoUrl: null,
          },
          archived: false,
        },
      },
      {
        id: "cached-prom",
        stackId: "stack-infra",
        stackName: "infra",
        serviceId: "svc-infra-prom",
        serviceName: "prometheus",
        imageRef: "quay.io/prometheus/prometheus:v2.52.0",
        groupName: "Cached Tools",
        title: "Cached Prometheus",
        description: "Cached metrics",
        href: "https://cached-metrics.example.com",
        icon: "prometheus.svg",
        status: "ok",
        isDockrev: false,
        service: {
          id: "svc-infra-prom",
          name: "prometheus",
          image: {
            ref: "quay.io/prometheus/prometheus:v2.52.0",
            tag: "v2.52.0",
            digest: "sha256:cached-prom",
            resolvedTag: "v2.52.0",
            resolvedTags: ["v2.52.0"],
          },
          homepage: {
            group: "Cached Tools",
            name: "Cached Prometheus",
            icon: "prometheus.svg",
            href: "https://cached-metrics.example.com",
            description: "Cached metrics",
          },
          candidate: null,
          ignore: null,
          versionInference: { status: "ready", reason: null, checkedAt: null },
          newVersionDiscoveryCount: null,
          settings: {
            autoRollback: true,
            backupTargets: { bindPaths: {}, volumeNames: {} },
            repoUrl: null,
          },
          archived: false,
        },
      },
    ],
  };
}
