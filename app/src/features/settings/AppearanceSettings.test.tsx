import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { AppSettings } from "../../model";
import { AppearanceSettings } from "./AppearanceSettings";
import { DEFAULT_APPEARANCE_SETTINGS } from "./appearance";

const settings: AppSettings = {
  ...DEFAULT_APPEARANCE_SETTINGS,
  previewText: true,
  previewImages: false,
  previewFileNames: false,
  allowText: true,
  allowHtml: true,
  allowImages: true,
  allowUrls: true,
  allowFiles: false,
  allowPrivate: false,
};

describe("AppearanceSettings", () => {
  it("renders labeled shared controls and emits field updates", () => {
    const onUpdate = vi.fn();
    render(<AppearanceSettings settings={settings} platform="desktop" onUpdate={onUpdate} />);

    expect(screen.getByRole("heading", { name: "外观与液态玻璃" })).toBeInTheDocument();
    expect(screen.getByLabelText("主题")).toBeInTheDocument();
    expect(screen.getByLabelText("窗口不透明度")).toHaveAttribute("min", "0.72");
    expect(screen.getByLabelText("玻璃模糊")).toHaveAttribute("max", "56");
    expect(screen.getByLabelText("玻璃饱和度")).toHaveAttribute("step", "0.05");
    expect(screen.getByLabelText("窗口圆角")).toBeInTheDocument();
    expect(screen.getByLabelText("高光强度")).toBeInTheDocument();
    expect(screen.getByLabelText("自定义强调色")).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("主题"), { target: { value: "dark" } });
    fireEvent.change(screen.getByLabelText("窗口不透明度"), { target: { value: "0.8" } });
    fireEvent.click(screen.getByRole("button", { name: "强调色预设 2" }));

    expect(onUpdate).toHaveBeenNthCalledWith(1, { theme: "dark" });
    expect(onUpdate).toHaveBeenNthCalledWith(2, { windowOpacity: 0.8 });
    expect(onUpdate).toHaveBeenNthCalledWith(3, { accentColor: "#5b7cfa" });
  });

  it("resets every appearance field to defaults", () => {
    const onUpdate = vi.fn();
    render(<AppearanceSettings settings={{ ...settings, theme: "dark", floatingOrbEnabled: true }} platform="desktop" onUpdate={onUpdate} />);

    fireEvent.click(screen.getByRole("button", { name: "恢复默认外观" }));

    expect(onUpdate).toHaveBeenCalledWith(DEFAULT_APPEARANCE_SETTINGS);
  });

  it("expands three-digit accent colors for the native color input", () => {
    render(<AppearanceSettings settings={{ ...settings, accentColor: "#abc" }} platform="desktop" onUpdate={vi.fn()} />);

    expect(screen.getByLabelText("自定义强调色")).toHaveValue("#aabbcc");
  });

  it("shows and updates the floating orb only on desktop", () => {
    const onUpdate = vi.fn();
    const { rerender } = render(<AppearanceSettings settings={settings} platform="desktop" onUpdate={onUpdate} />);

    fireEvent.click(screen.getByRole("switch", { name: "桌面悬浮球" }));
    expect(onUpdate).toHaveBeenCalledWith({ floatingOrbEnabled: true });

    rerender(<AppearanceSettings settings={settings} platform="android" onUpdate={onUpdate} />);
    expect(screen.queryByRole("switch", { name: "桌面悬浮球" })).not.toBeInTheDocument();
    expect(screen.getByLabelText("主题")).toBeInTheDocument();
    expect(screen.getByLabelText("玻璃模糊")).toBeInTheDocument();
  });
});
