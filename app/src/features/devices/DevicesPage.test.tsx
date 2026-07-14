import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DemoDesktopClient } from "../../ipc/demo-client";
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
});
