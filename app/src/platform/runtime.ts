import type { PlatformKind } from "../model";

export const detectPlatform = (): PlatformKind => {
  const override = import.meta.env.VITE_PLATFORM;
  if (override === "android") return "android";
  return /Android/i.test(navigator.userAgent) ? "android" : "desktop";
};
