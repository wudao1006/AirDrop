import type { AppSettings } from "../../model";

export type AppearanceSettings = Pick<
  AppSettings,
  | "theme"
  | "accentColor"
  | "windowOpacity"
  | "blurStrength"
  | "glassSaturation"
  | "cornerRadius"
  | "highlightStrength"
  | "floatingOrbEnabled"
>;

export const APPEARANCE_STORAGE_KEY = "airdrop.appearance.v1";

export const DEFAULT_APPEARANCE_SETTINGS: AppearanceSettings = {
  theme: "system",
  accentColor: "#168fae",
  windowOpacity: 0.94,
  blurStrength: 30,
  glassSaturation: 1.3,
  cornerRadius: 22,
  highlightStrength: 0.28,
  floatingOrbEnabled: false,
};

const HEX_COLOR = /^#(?:[\da-f]{3}|[\da-f]{6})$/i;

const clampNumber = (value: unknown, minimum: number, maximum: number, fallback: number): number => {
  if (typeof value !== "number" || !Number.isFinite(value)) return fallback;
  return Math.min(maximum, Math.max(minimum, value));
};

export const normalizeAppearanceSettings = (value: unknown): AppearanceSettings => {
  const candidate = value !== null && typeof value === "object" ? value as Record<string, unknown> : {};
  const accentColor = typeof candidate.accentColor === "string" ? candidate.accentColor.trim().toLowerCase() : "";

  return {
    theme: candidate.theme === "light" || candidate.theme === "dark" || candidate.theme === "system"
      ? candidate.theme
      : DEFAULT_APPEARANCE_SETTINGS.theme,
    accentColor: HEX_COLOR.test(accentColor) ? accentColor : DEFAULT_APPEARANCE_SETTINGS.accentColor,
    windowOpacity: clampNumber(candidate.windowOpacity, 0.72, 1, DEFAULT_APPEARANCE_SETTINGS.windowOpacity),
    blurStrength: clampNumber(candidate.blurStrength, 12, 56, DEFAULT_APPEARANCE_SETTINGS.blurStrength),
    glassSaturation: clampNumber(candidate.glassSaturation, 0.9, 1.6, DEFAULT_APPEARANCE_SETTINGS.glassSaturation),
    cornerRadius: clampNumber(candidate.cornerRadius, 14, 30, DEFAULT_APPEARANCE_SETTINGS.cornerRadius),
    highlightStrength: clampNumber(candidate.highlightStrength, 0, 0.6, DEFAULT_APPEARANCE_SETTINGS.highlightStrength),
    floatingOrbEnabled: typeof candidate.floatingOrbEnabled === "boolean"
      ? candidate.floatingOrbEnabled
      : DEFAULT_APPEARANCE_SETTINGS.floatingOrbEnabled,
  };
};

export const extractAppearanceSettings = (settings: AppSettings): AppearanceSettings => normalizeAppearanceSettings(settings);

const parseHexColor = (color: string): [number, number, number] => {
  const digits = color.slice(1);
  const expanded = digits.length === 3 ? [...digits].map((digit) => digit + digit).join("") : digits;
  return [
    Number.parseInt(expanded.slice(0, 2), 16),
    Number.parseInt(expanded.slice(2, 4), 16),
    Number.parseInt(expanded.slice(4, 6), 16),
  ];
};

const mixChannel = (source: number, target: number, amount: number): number =>
  Math.round(source + (target - source) * amount);

const mixHexColor = (color: string, target: number, amount: number): string =>
  `#${parseHexColor(color)
    .map((channel) => mixChannel(channel, target, amount).toString(16).padStart(2, "0"))
    .join("")}`;

const prefersDarkAppearance = (theme: AppearanceSettings["theme"]): boolean => {
  if (theme !== "system") return theme === "dark";
  return typeof window !== "undefined"
    && typeof window.matchMedia === "function"
    && window.matchMedia("(prefers-color-scheme: dark)").matches;
};

export const applyAppearanceSettings = (settings: AppearanceSettings): void => {
  if (typeof document === "undefined") return;

  const appearance = normalizeAppearanceSettings(settings);
  const accentChannels = parseHexColor(appearance.accentColor);
  const dark = prefersDarkAppearance(appearance.theme);
  const root = document.documentElement;
  root.dataset.theme = appearance.theme === "system" ? "" : appearance.theme;

  root.style.setProperty("--accent", appearance.accentColor);
  root.style.setProperty("--accent-hover", mixHexColor(appearance.accentColor, dark ? 255 : 0, dark ? 0.16 : 0.12));
  root.style.setProperty("--accent-soft", `rgba(${accentChannels.join(", ")}, ${dark ? 0.22 : 0.14})`);
  root.style.setProperty("--accent-text", mixHexColor(appearance.accentColor, dark ? 255 : 0, dark ? 0.35 : 0.18));
  root.style.setProperty("--window-opacity", String(appearance.windowOpacity));
  root.style.setProperty("--glass-blur", `${appearance.blurStrength}px`);
  root.style.setProperty("--glass-saturation", String(appearance.glassSaturation));
  root.style.setProperty("--user-radius", `${appearance.cornerRadius}px`);
  root.style.setProperty("--glass-highlight-opacity", String(appearance.highlightStrength));
};

export const subscribeToSystemAppearanceChanges = (settings: AppearanceSettings): (() => void) => {
  const appearance = normalizeAppearanceSettings(settings);
  if (appearance.theme !== "system" || typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return () => undefined;
  }

  const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
  const reapplyAppearance = () => applyAppearanceSettings(appearance);

  if (typeof mediaQuery.addEventListener === "function") {
    mediaQuery.addEventListener("change", reapplyAppearance);
    return () => mediaQuery.removeEventListener("change", reapplyAppearance);
  }

  if (typeof mediaQuery.addListener === "function") {
    mediaQuery.addListener(reapplyAppearance);
    return () => mediaQuery.removeListener(reapplyAppearance);
  }

  return () => undefined;
};

export const loadAppearanceSettings = (): AppearanceSettings => {
  if (typeof window === "undefined") return { ...DEFAULT_APPEARANCE_SETTINGS };

  try {
    const stored = window.localStorage?.getItem(APPEARANCE_STORAGE_KEY);
    return stored === null ? { ...DEFAULT_APPEARANCE_SETTINGS } : normalizeAppearanceSettings(JSON.parse(stored));
  } catch {
    return { ...DEFAULT_APPEARANCE_SETTINGS };
  }
};

export const saveAppearanceSettings = (settings: AppearanceSettings): void => {
  if (typeof window === "undefined") return;

  try {
    window.localStorage?.setItem(APPEARANCE_STORAGE_KEY, JSON.stringify(normalizeAppearanceSettings(settings)));
  } catch {
    // Persistence is best-effort; in-memory settings remain usable when storage is unavailable.
  }
};
