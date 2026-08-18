import type { Meta, StoryObj } from "@storybook/react";
import { ServiceDetailPage } from "../../pages/ServiceDetailPage";
import { withDockrevMockApi } from "../mocks/withDockrevMockApi";
import { buildServiceLogsSsePayload } from "../mocks/dockrevMockApi/shared";
import { buildLongLogsSnapshot } from "./serviceDetailPageStoryFixtures";
import { render } from "./serviceDetailStoryShared";

const meta: Meta<typeof ServiceDetailPage> = {
  title: "Pages/Service Log Follow",
  component: ServiceDetailPage,
  decorators: [withDockrevMockApi],
  parameters: { layout: "fullscreen" },
};

export default meta;
type Story = StoryObj<typeof ServiceDetailPage>;

function buildFollowTailPayload(id: number, marker: string) {
  const raw = [
    marker,
    ...Array.from({ length: 24 }, (_, index) => `trace detail ${index + 1}`),
  ].join("\n");
  return buildServiceLogsSsePayload([
    {
      type: "line",
      id,
      serviceId: "svc-prod-api",
      line: {
        ts: "2026-06-29T08:33:21.000Z",
        raw,
        plain: raw,
      },
    },
  ]);
}

export const FollowsAfterBufferEviction: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceLogsByServiceId: {
      "svc-prod-api": {
        snapshot: buildLongLogsSnapshot("svc-prod-api", 2000),
        eventsGate: "follow-after-buffer-eviction",
        eventsPayload: buildFollowTailPayload(2001, "follow-after-buffer-eviction"),
      },
    },
  },
  render: render("stack-prod", "svc-prod-api", "logs", "满缓冲时跳到最新后继续贴住新增日志"),
};

export const FollowsAfterAppend: Story = {
  parameters: {
    dockrevApiScenario: "dashboard-demo",
    dockrevServiceLogsByServiceId: {
      "svc-prod-api": {
        snapshot: buildLongLogsSnapshot("svc-prod-api", 100),
        eventsGate: "follow-after-append",
        eventsPayload: buildFollowTailPayload(101, "follow-after-append"),
      },
    },
  },
  render: render("stack-prod", "svc-prod-api", "logs", "普通追加时跳到最新后继续贴住新增日志"),
};
