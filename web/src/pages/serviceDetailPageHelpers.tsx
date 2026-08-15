import type {
  BackupTargetPolicy,
  Service,
  ServiceBackupTargetItem,
  ServiceBackupTargetsResponse,
  StackDetail,
} from "../api";
import { isDockrevImageRef } from "../runtimeConfig";
import { serviceRowStatus } from "../updateStatus";

export function svcBadge(svc: Service): string {
  const st = serviceRowStatus(svc);
  if (st === "blocked") return "被阻止";
  if (st === "archMismatch") return "架构不匹配";
  if (st === "hint") return "需确认";
  if (st === "updatable") return "可更新";
  return "无候选";
}

export function isDockrevService(svc: Service): boolean {
  return isDockrevImageRef(svc.image.ref);
}

export type ServiceDetailSection =
  | "overview"
  | "versions"
  | "history"
  | "monitoring"
  | "backup"
  | "logs"
  | "settings";

export function serviceDetailSectionLabel(section: ServiceDetailSection): string {
  if (section === "versions") return "版本";
  if (section === "history") return "更新记录";
  if (section === "monitoring") return "监控";
  if (section === "backup") return "备份";
  if (section === "logs") return "日志";
  if (section === "settings") return "设置";
  return "概览";
}

export type BackupTargetDraftItem = {
  key: string;
  policy: BackupTargetPolicy;
  relatedServiceCount: number;
  relatedServiceIds: string[];
};

export type BackupTargetsDraft = {
  bindPaths: BackupTargetDraftItem[];
  volumeNames: BackupTargetDraftItem[];
};

export function createBackupTargetsDraft(data: ServiceBackupTargetsResponse | null): BackupTargetsDraft {
  const normalize = (items: ServiceBackupTargetItem[]): BackupTargetDraftItem[] =>
    items.map((item) => ({
      key: item.key,
      policy: item.policy,
      relatedServiceCount: item.relatedServiceCount,
      relatedServiceIds: item.relatedServiceIds,
    }));
  return {
    bindPaths: normalize(data?.bindPaths ?? []),
    volumeNames: normalize(data?.volumeNames ?? []),
  };
}

function backupTargetRequestItems(items: BackupTargetDraftItem[]) {
  return items.map((item) => ({
    key: item.key,
    policy: item.policy,
  }));
}

export function backupTargetRequestFromDraft(draft: BackupTargetsDraft) {
  return {
    bindPaths: backupTargetRequestItems(draft.bindPaths),
    volumeNames: backupTargetRequestItems(draft.volumeNames),
  };
}

export function formatBackupRetentionSummary(storage: ServiceBackupTargetsResponse["storage"]): string {
  const hours = Math.round(storage.deleteAfterStableSeconds / 3600);
  return `目录 ${storage.baseDir} / 产物 .tar.zst / 最近 ${storage.keepLast} 份保留 / 其余稳定 ${hours}h 后清理`;
}

export function backupPolicyHint(item: BackupTargetDraftItem): string {
  if (item.policy === "disabled") return "当前服务不会为这个 target 触发自动备份";
  if (item.policy === "stop_related_services") {
    return item.relatedServiceCount > 1
      ? `备份前会协调停掉这 ${item.relatedServiceCount} 个关联服务，再恢复`
      : "备份前会先停掉当前服务，再恢复";
  }
  return item.relatedServiceCount > 1
    ? `保持这 ${item.relatedServiceCount} 个关联服务运行，直接备份`
    : "保持当前服务运行，直接备份";
}

export function backupRelationshipLabel(item: BackupTargetDraftItem): string {
  if (item.relatedServiceCount <= 1) return "关联 1 个服务";
  return `关联 ${item.relatedServiceCount} 个服务`;
}

export function ServiceDetailReadonlyBlocked(props: { title: string; detail: string }) {
  return (
    <div className="card serviceDetailReadonlyBlock">
      <div className="title">{props.title}</div>
      <div className="muted">{props.detail}</div>
    </div>
  );
}

export function sanitizeReadonlyStackSnapshot(stack: StackDetail): StackDetail {
  return {
    ...stack,
    services: stack.services.map((service) => ({
      ...service,
      settings: {
        autoRollback: false,
        backupTargets: {
          bindPaths: {},
          volumeNames: {},
        },
      },
    })),
  };
}
