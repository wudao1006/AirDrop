import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DemoDesktopClient } from "../ipc/demo-client";
import type { TelemetrySnapshot } from "../model";
import { App } from "./App";

describe("global request visibility", () => {
  it("shows a sync-group invitation outside the groups page and navigates to its actions", async () => {
    const demo = new DemoDesktopClient(async () => undefined);
    const snapshot = await demo.getSnapshot();
    snapshot.pendingGroupInvites = [{
      inviteId: "invite-1",
      groupId: "group-1",
      groupName: "个人设备",
      ownerDeviceId: "owner-1",
      ownerName: "工作电脑",
      expiresAt: new Date(Date.now() + 60_000).toISOString(),
    }];
    const client = Object.create(demo) as DemoDesktopClient;
    client.getSnapshot = vi.fn(async () => snapshot);
    client.subscribe = vi.fn(() => () => undefined);

    render(<App client={client} />);
    expect(await screen.findByText("收到同步组邀请")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /同步组/ })).toHaveTextContent("1");
    fireEvent.click(screen.getByRole("button", { name: "查看并处理" }));
    expect(await screen.findByRole("button", { name: "接受邀请" })).toBeInTheDocument();
  });

  it("keeps the pairing window visible after navigating away and back", async () => {
    const demo = new DemoDesktopClient(async () => undefined);
    const snapshot = await demo.getSnapshot();
    snapshot.pairingAllowedUntil = Math.floor(Date.now() / 1000) + 120;
    const client = Object.create(demo) as DemoDesktopClient;
    client.getSnapshot = vi.fn(async () => snapshot);
    client.subscribe = vi.fn(() => () => undefined);

    render(<App client={client} />);
    fireEvent.click(await screen.findByRole("button", { name: "设备" }));
    expect(screen.getByRole("button", { name: "配对窗口已开放" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.click(screen.getByRole("button", { name: "设备" }));
    expect(screen.getByRole("button", { name: "配对窗口已开放" })).toBeInTheDocument();
  });

  it("keeps a newer telemetry event when the initial request resolves late", async () => {
    const demo = new DemoDesktopClient(async () => undefined);
    const snapshot = await demo.getSnapshot();
    const older = await demo.getTelemetry();
    const deviceId = older.transfers[0].deviceId;
    older.sampledAt = "2026-07-15T00:00:00Z";
    older.transfers = [];
    const newer: TelemetrySnapshot = {
      ...structuredClone(older),
      sampledAt: "2026-07-15T00:00:01Z",
      transfers: [{
        id: "new-transfer",
        attemptId: 42,
        deviceId,
        direction: "download",
        kind: "text",
        totalBytes: 12,
        transferredBytes: 12,
        startedAt: "2026-07-15T00:00:01Z",
        completedAt: "2026-07-15T00:00:01Z",
        durationMs: 8,
        networkDurationMs: null,
        confirmationDurationMs: null,
        remoteProcessingMs: null,
        speedBps: 0,
        averageBps: 1_500,
        status: "success",
        message: "较新的传输记录",
      }],
    };
    let telemetryListener: ((value: TelemetrySnapshot) => void) | undefined;
    let resolveTelemetry: ((value: TelemetrySnapshot) => void) | undefined;
    const client = Object.create(demo) as DemoDesktopClient;
    client.getSnapshot = vi.fn(async () => snapshot);
    client.subscribe = vi.fn(() => () => undefined);
    client.subscribeTelemetry = vi.fn((listener) => {
      telemetryListener = listener;
      return () => undefined;
    });
    client.getTelemetry = vi.fn(() => new Promise<TelemetrySnapshot>((resolve) => {
      resolveTelemetry = resolve;
    }));
    client.setTelemetryObserving = vi.fn(async () => undefined);

    render(<App client={client} />);
    fireEvent.click(await screen.findByRole("button", { name: "传输" }));
    await waitFor(() => expect(telemetryListener).toBeDefined());
    act(() => telemetryListener?.(newer));
    expect(await screen.findByText("较新的传输记录")).toBeInTheDocument();
    await act(async () => {
      resolveTelemetry?.(older);
      await Promise.resolve();
    });
    expect(screen.getByText("较新的传输记录")).toBeInTheDocument();
  });
});
