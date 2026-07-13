import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DemoDesktopClient } from "../../ipc/demo-client";
import type { AppSettings } from "../../model";
import {
  APPEARANCE_STORAGE_KEY,
  DEFAULT_APPEARANCE_SETTINGS,
  applyAppearanceSettings,
  loadAppearanceSettings,
  normalizeAppearanceSettings,
  saveAppearanceSettings,
  subscribeToSystemAppearanceChanges,
} from "./appearance";

const settings: AppSettings = {
  ...DEFAULT_APPEARANCE_SETTINGS,
  previewText: false,
  previewImages: true,
  previewFileNames: true,
  allowText: false,
  allowHtml: false,
  allowImages: false,
  allowUrls: false,
  allowFiles: true,
  allowPrivate: true,
  globalShortcut: "Ctrl+Alt+KeyZ",
};

describe("appearance settings", () => {
  beforeEach(() => {
    window.localStorage.clear();
    document.documentElement.removeAttribute("data-theme");
    document.documentElement.removeAttribute("style");
  });

  afterEach(() => vi.unstubAllGlobals());

  it("uses defaults when no persisted value exists", () => {
    expect(loadAppearanceSettings()).toEqual(DEFAULT_APPEARANCE_SETTINGS);
  });

  it("falls back field by field for malformed values", () => {
    expect(normalizeAppearanceSettings({
      theme: "sepia",
      accentColor: "blue",
      windowOpacity: "0.8",
      blurStrength: Number.NaN,
      glassSaturation: null,
      cornerRadius: undefined,
      highlightStrength: Number.POSITIVE_INFINITY,
      floatingOrbEnabled: "yes",
    })).toEqual(DEFAULT_APPEARANCE_SETTINGS);

    window.localStorage.setItem(APPEARANCE_STORAGE_KEY, "not-json");
    expect(loadAppearanceSettings()).toEqual(DEFAULT_APPEARANCE_SETTINGS);
  });

  it("rejects hex colors with alpha channels", () => {
    expect(normalizeAppearanceSettings({ accentColor: "#0000" }).accentColor)
      .toBe(DEFAULT_APPEARANCE_SETTINGS.accentColor);
    expect(normalizeAppearanceSettings({ accentColor: "#11223344" }).accentColor)
      .toBe(DEFAULT_APPEARANCE_SETTINGS.accentColor);
  });

  it("clamps finite numeric values to their supported ranges", () => {
    expect(normalizeAppearanceSettings({
      windowOpacity: 0,
      blurStrength: 100,
      glassSaturation: 0.2,
      cornerRadius: 80,
      highlightStrength: -1,
    })).toMatchObject({
      windowOpacity: 0.72,
      blurStrength: 56,
      glassSaturation: 0.9,
      cornerRadius: 30,
      highlightStrength: 0,
    });
  });

  it("round trips valid appearance settings", () => {
    const valid: AppSettings = {
      ...settings,
      theme: "dark",
      accentColor: "#A1B2C3",
      windowOpacity: 0.82,
      blurStrength: 44,
      glassSaturation: 1.5,
      cornerRadius: 18,
      highlightStrength: 0.42,
      floatingOrbEnabled: true,
    };

    saveAppearanceSettings(valid);

    expect(loadAppearanceSettings()).toEqual({
      theme: "dark",
      accentColor: "#a1b2c3",
      windowOpacity: 0.82,
      blurStrength: 44,
      glassSaturation: 1.5,
      cornerRadius: 18,
      highlightStrength: 0.42,
      floatingOrbEnabled: true,
    });
  });

  it("does not persist clipboard policy or privacy settings", () => {
    saveAppearanceSettings(settings);

    const persisted = JSON.parse(window.localStorage.getItem(APPEARANCE_STORAGE_KEY) ?? "{}");
    expect(persisted).toEqual(DEFAULT_APPEARANCE_SETTINGS);
    expect(persisted).not.toHaveProperty("previewText");
    expect(persisted).not.toHaveProperty("allowText");
  });

  it("normalizes appearance updates before exposing the client snapshot", async () => {
    const client = new DemoDesktopClient(async () => undefined);

    await client.updateSettings({
      accentColor: "not-a-color",
      windowOpacity: 0,
      blurStrength: 100,
      previewText: false,
    });

    const snapshot = await client.getSnapshot();
    expect(snapshot.settings).toMatchObject({
      accentColor: DEFAULT_APPEARANCE_SETTINGS.accentColor,
      windowOpacity: 0.72,
      blurStrength: 56,
      previewText: false,
    });
  });

  it("applies normalized theme and live material variables to the document root", () => {
    applyAppearanceSettings({
      ...DEFAULT_APPEARANCE_SETTINGS,
      theme: "dark",
      accentColor: "#A1B2C3",
      windowOpacity: 0,
      blurStrength: 100,
      glassSaturation: 1.5,
      cornerRadius: 18,
      highlightStrength: 0.42,
    });

    const root = document.documentElement;
    expect(root.dataset.theme).toBe("dark");
    expect(root.style.getPropertyValue("--accent")).toBe("#a1b2c3");
    expect(root.style.getPropertyValue("--accent-hover")).toBe("#b0becd");
    expect(root.style.getPropertyValue("--accent-soft")).toBe("rgba(161, 178, 195, 0.22)");
    expect(root.style.getPropertyValue("--accent-text")).toBe("#c2cdd8");
    expect(root.style.getPropertyValue("--window-opacity")).toBe("0.72");
    expect(root.style.getPropertyValue("--glass-blur")).toBe("56px");
    expect(root.style.getPropertyValue("--glass-saturation")).toBe("1.5");
    expect(root.style.getPropertyValue("--user-radius")).toBe("18px");
    expect(root.style.getPropertyValue("--glass-highlight-opacity")).toBe("0.42");

    applyAppearanceSettings(DEFAULT_APPEARANCE_SETTINGS);
    expect(root.dataset.theme).toBe("");
  });

  it("derives concrete light-theme accent variants without color-mix", () => {
    applyAppearanceSettings({
      ...DEFAULT_APPEARANCE_SETTINGS,
      theme: "light",
      accentColor: "#A1B2C3",
    });

    const root = document.documentElement;
    expect(root.style.getPropertyValue("--accent-hover")).toBe("#8e9dac");
    expect(root.style.getPropertyValue("--accent-soft")).toBe("rgba(161, 178, 195, 0.14)");
    expect(root.style.getPropertyValue("--accent-text")).toBe("#8492a0");
    expect(root.getAttribute("style")).not.toContain("color-mix");
  });

  it("reapplies system accent variants on OS theme changes and cleans up", () => {
    const listeners = new Set<EventListener>();
    const mediaQuery = {
      matches: false,
      media: "(prefers-color-scheme: dark)",
      onchange: null,
      addEventListener: vi.fn((_type: string, listener: EventListener) => listeners.add(listener)),
      removeEventListener: vi.fn((_type: string, listener: EventListener) => listeners.delete(listener)),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    } as unknown as MediaQueryList;
    vi.stubGlobal("matchMedia", vi.fn(() => mediaQuery));

    const appearance = {
      ...DEFAULT_APPEARANCE_SETTINGS,
      theme: "system" as const,
      accentColor: "#a1b2c3",
    };
    applyAppearanceSettings(appearance);
    const unsubscribe = subscribeToSystemAppearanceChanges(appearance);

    expect(document.documentElement.style.getPropertyValue("--accent-hover")).toBe("#8e9dac");
    Object.defineProperty(mediaQuery, "matches", { configurable: true, value: true });
    listeners.forEach((listener) => listener(new Event("change")));
    expect(document.documentElement.style.getPropertyValue("--accent-hover")).toBe("#b0becd");

    unsubscribe();
    expect(mediaQuery.removeEventListener).toHaveBeenCalledWith("change", expect.any(Function));
    Object.defineProperty(mediaQuery, "matches", { configurable: true, value: false });
    listeners.forEach((listener) => listener(new Event("change")));
    expect(document.documentElement.style.getPropertyValue("--accent-hover")).toBe("#b0becd");
  });

  it("is safe when rendered without a browser document", () => {
    vi.stubGlobal("document", undefined);
    expect(() => applyAppearanceSettings(DEFAULT_APPEARANCE_SETTINGS)).not.toThrow();
  });

  it("does not subscribe when rendered without a browser window", () => {
    vi.stubGlobal("window", undefined);
    expect(() => subscribeToSystemAppearanceChanges(DEFAULT_APPEARANCE_SETTINGS)()).not.toThrow();
  });
});
