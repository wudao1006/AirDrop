import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DemoDesktopClient } from "../../ipc/demo-client";
import type { TelemetrySnapshot } from "../../model";
import { DevicesPage } from "./DevicesPage";

describe("DevicesPage device aliases", () => {
  it("saves a one-way local alias", async () => {
    const client = new DemoDesktopClient(async () => undefined);
    const snapshot = await client.getSnapshot();
    snapshot.trustedDevices = [{
      deviceId: "peer-1",
      deviceName: "Work PC",
      advertisedName: "Work PC",
      localAlias: null,
      platform: "windows",
      pairedAt: new Date().toISOString(),
      online: true,
      syncEnabled: true,
    }];
    const save = vi.spyOn(client, "setDeviceAlias").mockResolvedValue();
    render(<DevicesPage snapshot={snapshot} client={client} onError={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: "备注名" }));
    fireEvent.change(screen.getByLabelText("为 Work PC 设置本地备注名"), { target: { value: "办公室电脑" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => expect(save).toHaveBeenCalledWith("peer-1", "办公室电脑"));
  });

  it("shows live connection diagnostics and copies a redacted report", async () => {
    const client = new DemoDesktopClient(async () => undefined);
    const snapshot = await client.getSnapshot();
    snapshot.trustedDevices = [{
      deviceId: "peer-1",
      deviceName: "Work PC",
      advertisedName: "Work PC",
      localAlias: null,
      platform: "windows",
      pairedAt: new Date().toISOString(),
      online: true,
      syncEnabled: true,
    }];
    const telemetry: TelemetrySnapshot = {
      sampledAt: new Date().toISOString(),
      peers: [{
        deviceId: "peer-1",
        connected: true,
        rttMs: 16,
        uploadBps: 2_000,
        downloadBps: 4_000,
        recentUploadBps: 1_800,
        recentDownloadBps: 3_600,
        lossPercent: 0.25,
        totalUploadedBytes: 10_000,
        totalDownloadedBytes: 20_000,
        connectedAt: new Date(Date.now() - 60_000).toISOString(),
        lastActivityAt: new Date().toISOString(),
        reconnectCount: 2,
        lastDisconnectReason: null,
        lastDisconnectCode: null,
        lastDisconnectedAt: null,
        lastDisconnectPlanned: false,
        unexpectedDisconnectCount: 1,
      }],
      transfers: [],
    };
    const copy = vi.spyOn(client, "copyDiagnosticReport").mockResolvedValue();
    render(<DevicesPage snapshot={snapshot} telemetry={telemetry} client={client} onError={vi.fn()} />);

    expect(screen.getByText(/16 ms/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "连接详情" }));
    expect(screen.getByText("0.25%")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "复制诊断摘要" }));

    await waitFor(() => expect(copy).toHaveBeenCalledWith(expect.stringContaining("设备：Work PC")));
    expect(copy.mock.calls[0][0]).not.toContain("剪贴板正文");
  });
});
