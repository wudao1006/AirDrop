import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DemoDesktopClient } from "../../ipc/demo-client";
import { SettingsPage } from "./SettingsPage";

describe("SettingsPage shortcut recorder", () => {
  it("shows Ctrl+Alt+Z by default and saves a custom modified key", async () => {
    const client = new DemoDesktopClient(async () => undefined);
    const snapshot = await client.getSnapshot();
    const save = vi.spyOn(client, "setGlobalShortcut");
    render(<SettingsPage snapshot={snapshot} client={client} onError={vi.fn()} />);

    expect(screen.getByText("Z")).toBeInTheDocument();
    const recorder = screen.getByRole("button", { name: "自定义全局快捷键" });
    fireEvent.click(recorder);
    fireEvent.keyDown(recorder, { key: "X", code: "KeyX", ctrlKey: true, altKey: true });

    await waitFor(() => expect(save).toHaveBeenCalledWith("Ctrl+Alt+KeyX"));
  });
});
