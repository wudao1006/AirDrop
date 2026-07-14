import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";
import { DemoDesktopClient } from "../../ipc/demo-client";

describe("explicit clipboard import", () => {
  it("renders as a desktop application shell without implementation notices", async () => {
    const client = new DemoDesktopClient(async () => undefined);

    render(<App client={client} />);

    expect(await screen.findByRole("heading", { name: "概览" })).toBeInTheDocument();
    expect(screen.queryByText(/演示|Rust|Daemon/)).not.toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "主导航" })).toBeInTheDocument();
    expect(screen.queryByText("可用槽位")).not.toBeInTheDocument();
  });

  it("does not write the system clipboard when remote slots are loaded", async () => {
    const writer = vi.fn(async () => undefined);
    const client = new DemoDesktopClient(writer);

    const snapshot = await client.getSnapshot();

    expect(snapshot.slots.length).toBeGreaterThan(0);
    expect(writer).not.toHaveBeenCalled();
  });

  it("models Android suspension and resume without reading or writing the clipboard", async () => {
    const writer = vi.fn(async () => undefined);
    const reader = vi.fn(async () => "phone clipboard");
    const client = new DemoDesktopClient(writer, reader, "android");

    await client.setAppActivity("background");
    expect((await client.getSnapshot()).activity).toBe("suspended");
    await expect(client.createImportIntent("macbook-slot", 7)).rejects.toThrow("回到前台");
    await client.setAppActivity("foreground");
    expect((await client.getSnapshot()).activity).toBe("foreground_live");
    expect(reader).not.toHaveBeenCalled();
    expect(writer).not.toHaveBeenCalled();
  });

  it("publishes the current clipboard only after an explicit foreground command", async () => {
    const reader = vi.fn(async () => "Android foreground text");
    const client = new DemoDesktopClient(async () => undefined, reader, "android");

    await client.publishCurrentClipboard();

    expect(reader).toHaveBeenCalledOnce();
    expect((await client.getSnapshot()).lastPublishedPreview).toContain("Android foreground text");
  });

  it("allows an explicit clipboard capability retry after Android denies access", async () => {
    const reader = vi.fn()
      .mockRejectedValueOnce(new Error("clipboard permission denied"))
      .mockResolvedValueOnce("permission granted text");
    const client = new DemoDesktopClient(async () => undefined, reader, "android");

    await expect(client.publishCurrentClipboard()).rejects.toThrow("permission denied");
    expect((await client.getSnapshot()).clipboardCapability.canReadText).toBe(false);
    await client.publishCurrentClipboard();

    expect(reader).toHaveBeenCalledTimes(2);
    expect((await client.getSnapshot()).clipboardCapability.canReadText).toBe(true);
  });

  it("renders Android touch navigation and foreground status", async () => {
    const client = new DemoDesktopClient(async () => undefined, async () => "", "android");

    render(<App client={client} />);

    expect(await screen.findByRole("heading", { name: "概览" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "刷新本机剪贴板" })).toBeInTheDocument();
    expect(screen.getAllByText("前台实时").length).toBeGreaterThan(0);
    expect(screen.queryByRole("button", { name: "同步组" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "传输" })).not.toBeInTheDocument();
  });

  it("writes only after the user creates and confirms a ready import", async () => {
    const writer = vi.fn(async () => undefined);
    const client = new DemoDesktopClient(writer);
    const snapshot = await client.getSnapshot();
    const slot = snapshot.slots.find((item) => item.id === "macbook-slot");
    expect(slot).toBeDefined();

    const importId = await client.createImportIntent(slot!.id, slot!.revision);
    expect(writer).not.toHaveBeenCalled();

    await client.confirmImport(importId);
    expect(writer).toHaveBeenCalledOnce();
    expect(writer).toHaveBeenCalledWith("这段文字来自 MacBook，可以选择后写入当前设备。");
  });

  it("writes a ready remote slot after one explicit use click", async () => {
    const writer = vi.fn(async () => undefined);
    const client = new DemoDesktopClient(writer);

    render(<App client={client} />);

    fireEvent.click(await screen.findByRole("button", { name: "使用" }));

    await waitFor(() => expect(writer).toHaveBeenCalledOnce());
    expect(writer).toHaveBeenCalledWith("这段文字来自 MacBook，可以选择后写入当前设备。");
  });

  it("rejects stale UI revisions instead of silently selecting newer content", async () => {
    const client = new DemoDesktopClient(async () => undefined);

    await expect(client.createImportIntent("macbook-slot", 1)).rejects.toThrow("设备槽位已经更新");
  });
});
