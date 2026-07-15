import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { DemoDesktopClient } from "../../ipc/demo-client";
import type { TelemetrySnapshot } from "../../model";
import { TransfersPage } from "./TransfersPage";

describe("TransfersPage", () => {
  it("renders active speed, progress and recent results", async () => {
    const client = new DemoDesktopClient(async () => undefined);
    const snapshot = await client.getSnapshot();
    snapshot.trustedDevices = [{
      deviceId: "peer-1",
      deviceName: "工作电脑",
      advertisedName: "工作电脑",
      localAlias: null,
      platform: "windows",
      pairedAt: new Date().toISOString(),
      online: true,
      syncEnabled: true,
    }];
    const telemetry: TelemetrySnapshot = {
      sampledAt: new Date().toISOString(),
      peers: [],
      transfers: [{
        id: "active-1",
        attemptId: 1,
        deviceId: "peer-1",
        direction: "upload",
        kind: "files",
        totalBytes: 1_000,
        transferredBytes: 500,
        startedAt: new Date().toISOString(),
        completedAt: null,
        durationMs: 500,
        networkDurationMs: null,
        confirmationDurationMs: null,
        remoteProcessingMs: null,
        speedBps: 1_000,
        averageBps: 1_000,
        status: "active",
        message: null,
      }, {
        id: "done-1",
        attemptId: 2,
        deviceId: "peer-1",
        direction: "download",
        kind: "text",
        totalBytes: 80,
        transferredBytes: 80,
        startedAt: new Date().toISOString(),
        completedAt: new Date().toISOString(),
        durationMs: 240,
        networkDurationMs: 80,
        confirmationDurationMs: 160,
        remoteProcessingMs: 12,
        speedBps: 0,
        averageBps: 333,
        status: "success",
        message: "已写入设备槽位",
      }, {
        id: "sent-1",
        attemptId: 3,
        deviceId: "peer-1",
        direction: "upload",
        kind: "image",
        totalBytes: 256,
        transferredBytes: 256,
        startedAt: new Date().toISOString(),
        completedAt: new Date().toISOString(),
        durationMs: 120,
        networkDurationMs: 120,
        confirmationDurationMs: 0,
        remoteProcessingMs: null,
        speedBps: 0,
        averageBps: 2_133,
        status: "sent",
        message: "对端版本不支持接收确认",
      }],
    };

    render(<TransfersPage snapshot={snapshot} telemetry={telemetry} />);

    expect(screen.getByText("50% · 1000 B/s · 预计剩余 1 秒")).toBeInTheDocument();
    expect(screen.getByText(/接收自 工作电脑/)).toBeInTheDocument();
    expect(screen.getByText("已写入设备槽位")).toBeInTheDocument();
    expect(screen.getByText("已发送")).toBeInTheDocument();
  });
});
