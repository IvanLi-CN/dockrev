import type { Meta } from "@storybook/react";
import { ServiceDetailPage } from "../../pages/ServiceDetailPage";
import { withDockrevMockApi } from "../mocks/withDockrevMockApi";
import { expectStory, normalizeText, waitForCondition } from "./storyAssertions";
import { findSectionCard, findTab, render, type ServiceDetailStory } from "./serviceDetailStoryShared";

const meta: Meta<typeof ServiceDetailPage> = {
  title: "Pages/ServiceDetailPage",
  component: ServiceDetailPage,
  decorators: [withDockrevMockApi],
  tags: ["autodocs"],
  parameters: { layout: "fullscreen" },
};

export default meta;
type Story = ServiceDetailStory;

export const BackupRecordsCleanupStates: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceBackupTargetsById: {
      "svc-prod-api": {
        bindPaths: [],
        volumeNames: [],
        storage: {
          baseDir: "/srv/dockrev/backups",
          artifactPattern: "/srv/dockrev/backups/<stackId>/<timestamp>.tar.zst",
          compression: "zstd",
          keepLast: 0,
          deleteAfterStableSeconds: 3600,
        },
      },
    },
    dockrevServiceBackupRecordsById: {
      "svc-prod-api": {
        records: [
          {
            backupId: "bkp_cleanup-delayed",
            jobId: "job_cleanup-delayed",
            scope: "service",
            status: "success",
            createdAt: "2026-07-28T08:00:00.000Z",
            finishedAt: "2026-07-28T08:00:04.000Z",
            artifactPath: "/srv/dockrev/backups/stack-prod/20260728-080000.tar.zst",
            sizeBytes: 12_000_000,
            cleanupAfter: "2026-08-01T08:00:00.000Z",
            lastCleanupAttemptAt: "2026-08-27T08:10:00.000Z",
            lastCleanupError: "managed storage temporarily unavailable",
            deletedAt: null,
            missingAt: null,
            error: null,
            assets: [],
          },
          {
            backupId: "bkp_cleanup-deleted",
            jobId: "job_cleanup-deleted",
            scope: "stack",
            status: "success",
            createdAt: "2026-07-20T08:00:00.000Z",
            finishedAt: "2026-07-20T08:00:04.000Z",
            artifactPath: "/srv/dockrev/backups/stack-prod/20260720-080000.tar.zst",
            sizeBytes: 10_000_000,
            cleanupAfter: "2026-07-21T08:00:00.000Z",
            deletedAt: "2026-08-27T08:11:00.000Z",
            error: null,
            assets: [],
          },
          {
            backupId: "bkp_cleanup-missing",
            jobId: "job_cleanup-missing",
            scope: "service",
            status: "success",
            createdAt: "2026-07-19T08:00:00.000Z",
            finishedAt: "2026-07-19T08:00:04.000Z",
            artifactPath: "/srv/dockrev/backups/stack-prod/20260719-080000.tar.zst",
            sizeBytes: 9_000_000,
            cleanupAfter: "2026-07-20T08:00:00.000Z",
            missingAt: "2026-08-27T08:12:00.000Z",
            deletedAt: null,
            error: null,
            assets: [],
          },
        ],
      },
    },
  },
  render: render("stack-prod", "svc-prod-api", "backup", "清理状态按已删除、已核实缺失与延迟分别呈现"),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, "backup-records")));
    await waitForCondition(() => canvasElement.querySelectorAll("[data-service-backup-record-status]").length === 3);
    const text = normalizeText(canvasElement.textContent);
    expectStory(text.includes("清理延迟"), "cleanup delayed status missing");
    expectStory(text.includes("已删除"), "deleted status missing");
    expectStory(text.includes("文件已缺失（已核实）"), "verified missing status missing");
    expectStory(text.includes("managed storage temporarily unavailable"), "cleanup error missing");
    expectStory(Boolean(canvasElement.querySelector('[data-slot="alert"] svg')), "cleanup delay Alert icon missing");
    expectStory(text.includes("删除时间"), "deleted timestamp label missing");
    expectStory(text.includes("核实缺失时间"), "missing timestamp label missing");
    const summary = canvasElement.querySelector<HTMLElement>('[data-service-detail-section-card="backup-summary"]');
    const records = canvasElement.querySelector<HTMLElement>('[data-service-detail-section-card="backup-records"]');
    expectStory(Boolean(summary && records && records.getBoundingClientRect().top - summary.getBoundingClientRect().bottom >= 15), "backup cards should keep a 16px gap");
    expectStory(findTab(canvasElement, "backup")?.getAttribute("data-state") === "active", "backup tab should stay active");
  },
};
