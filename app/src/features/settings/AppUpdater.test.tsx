import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { AppUpdater } from "./AppUpdater";
import type { AvailableUpdate, UpdaterAdapter } from "./updater-adapter";

describe("AppUpdater", () => {
  it("checks, downloads and installs a signed update from the settings page", async () => {
    const update: AvailableUpdate = {
      version: "0.2.0",
      notes: "修复悬浮球",
      install: vi.fn(async (onProgress) => {
        onProgress({ downloaded: 50, total: 100 });
        onProgress({ downloaded: 100, total: 100 });
      }),
      dispose: vi.fn(async () => undefined),
    };
    const adapter: UpdaterAdapter = {
      supported: true,
      getCurrentVersion: vi.fn(async () => "0.1.2"),
      check: vi.fn(async () => update),
    };
    render(<AppUpdater adapter={adapter} />);
    expect(await screen.findByText("当前 v0.1.2")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /检查更新/ }));
    expect(await screen.findByText("发现新版本 0.2.0")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /下载并安装/ }));
    await waitFor(() => expect(update.install).toHaveBeenCalledOnce());
    expect(screen.getByText("更新已下载，正在安装并重启…")).toBeInTheDocument();
  });
});
