import { Cpu, Download, HardDriveDownload, HardDriveUpload, MemoryStick, Upload } from "lucide-react";
import type { ReactNode } from "react";
import type { ServiceResourceSnapshot } from "../api";
import { computeMonitorTerminalRate, formatMonitorBytes, formatMonitorPercent, formatMonitorRate } from "../pages/serviceDetailMonitorHelpers";

type MonitorMetric = {
  label: string;
  value: string;
  icon: ReactNode;
};

export function ServiceTopbarMonitorSummary(props: { snapshot: ServiceResourceSnapshot | null }) {
  const latestSample = props.snapshot?.samples.at(-1) ?? null;
  const previousSample = props.snapshot?.samples.at(-2) ?? null;
  const diskReadRate = computeMonitorTerminalRate(previousSample, latestSample, (sample) => sample.blockReadBytes);
  const diskWriteRate = computeMonitorTerminalRate(previousSample, latestSample, (sample) => sample.blockWriteBytes);
  const rxRate = computeMonitorTerminalRate(previousSample, latestSample, (sample) => sample.netRxBytes);
  const txRate = computeMonitorTerminalRate(previousSample, latestSample, (sample) => sample.netTxBytes);
  const groups: { key: string; metrics: MonitorMetric[] }[] = [
    {
      key: "compute",
      metrics: [
        { label: "CPU", value: formatMonitorPercent(latestSample?.cpuPercent ?? null), icon: <Cpu className="svcDetailMonitorGlyph" aria-hidden="true" /> },
        { label: "内存", value: formatMonitorBytes(latestSample?.memUsedBytes ?? null), icon: <MemoryStick className="svcDetailMonitorGlyph" aria-hidden="true" /> },
      ],
    },
    {
      key: "disk",
      metrics: [
        { label: "磁盘读", value: formatMonitorRate(diskReadRate), icon: <HardDriveDownload className="svcDetailMonitorGlyph" aria-hidden="true" /> },
        { label: "磁盘写", value: formatMonitorRate(diskWriteRate), icon: <HardDriveUpload className="svcDetailMonitorGlyph" aria-hidden="true" /> },
      ],
    },
    {
      key: "network",
      metrics: [
        { label: "下载", value: formatMonitorRate(rxRate), icon: <Download className="svcDetailMonitorGlyph" aria-hidden="true" /> },
        { label: "上传", value: formatMonitorRate(txRate), icon: <Upload className="svcDetailMonitorGlyph" aria-hidden="true" /> },
      ],
    },
  ];

  return (
    <div className="topbarServiceMonitorSummary" data-service-detail-context="monitor-summary" aria-label="服务监控指标">
      {groups.map((group) => (
        <div key={group.key} className="topbarServiceMonitorGroup" data-monitor-group={group.key}>
          {group.metrics.map((metric) => (
            <div
              key={metric.label}
              className="topbarServiceMonitorMetric"
              data-monitor-metric={metric.label}
              aria-label={`${metric.label} ${metric.value}`}
              title={`${metric.label} ${metric.value}`}
            >
              <span className="topbarServiceMonitorMetricIcon" aria-hidden="true">
                {metric.icon}
              </span>
              <span className="topbarServiceMonitorMetricValue">{metric.value}</span>
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}
