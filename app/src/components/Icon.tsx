import type { SVGProps } from "react";

export type IconName =
  | "logo" | "home" | "clipboard" | "devices" | "groups" | "transfer" | "settings"
  | "search" | "check" | "clock" | "download" | "shield" | "alert" | "chevron"
  | "pause" | "play" | "copy" | "monitor" | "apple" | "windows" | "linux"
  | "text" | "image" | "link" | "files" | "code" | "x" | "sparkles" | "plus" | "phone" | "refresh";

const paths: Record<IconName, React.ReactNode> = {
  logo: <><path d="M12 3v18M3 12h18"/><circle cx="12" cy="12" r="4"/><path d="M5.6 5.6l12.8 12.8M18.4 5.6 5.6 18.4"/></>,
  home: <><path d="m3 11 9-8 9 8"/><path d="M5 10v10h14V10M9 20v-6h6v6"/></>,
  clipboard: <><rect x="5" y="4" width="14" height="17" rx="2"/><path d="M9 4.5V3h6v1.5M8 10h8M8 14h6"/></>,
  devices: <><rect x="3" y="4" width="14" height="11" rx="2"/><path d="M8 20h10M10 15v5"/><rect x="16" y="8" width="5" height="10" rx="1"/></>,
  groups: <><circle cx="9" cy="8" r="3"/><circle cx="17" cy="9" r="2.5"/><path d="M3 20c0-4 2-6 6-6s6 2 6 6M15 15c3.5 0 5 1.7 5 4.5"/></>,
  transfer: <><path d="M4 7h14M14 3l4 4-4 4M20 17H6M10 13l-4 4 4 4"/></>,
  settings: <><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1-2.8 2.8-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.6v.2h-4V21a1.7 1.7 0 0 0-1-1.6 1.7 1.7 0 0 0-1.9.3l-.1.1L4.2 17l.1-.1a1.7 1.7 0 0 0 .3-1.9A1.7 1.7 0 0 0 3 14H3v-4h.1a1.7 1.7 0 0 0 1.5-1 1.7 1.7 0 0 0-.3-1.9L4.2 7 7 4.2l.1.1a1.7 1.7 0 0 0 1.9.3A1.7 1.7 0 0 0 10 3V3h4v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.9-.3l.1-.1L19.8 7l-.1.1a1.7 1.7 0 0 0-.3 1.9 1.7 1.7 0 0 0 1.6 1h.2v4H21a1.7 1.7 0 0 0-1.6 1Z"/></>,
  search: <><circle cx="11" cy="11" r="7"/><path d="m20 20-4-4"/></>,
  check: <path d="m5 12 4 4L19 6"/>, clock: <><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></>,
  download: <><path d="M12 3v12M7 10l5 5 5-5M4 21h16"/></>,
  shield: <><path d="M12 3 4.5 6v5.5c0 4.7 3.1 7.9 7.5 9.5 4.4-1.6 7.5-4.8 7.5-9.5V6L12 3Z"/><path d="m9 12 2 2 4-5"/></>,
  alert: <><path d="M12 3 2.8 20h18.4L12 3Z"/><path d="M12 9v5M12 17h.01"/></>,
  chevron: <path d="m9 18 6-6-6-6"/>, pause: <><path d="M8 5v14M16 5v14"/></>, play: <path d="m8 5 11 7-11 7V5Z"/>,
  copy: <><rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h3"/></>,
  monitor: <><rect x="3" y="4" width="18" height="13" rx="2"/><path d="M8 21h8M12 17v4"/></>,
  apple: <><path d="M15 8c-1.4 0-2.2.8-3.2.8S9.8 8 8.4 8C5.7 8 4 10.4 4 13c0 3.7 2.7 8 4.8 8 1 0 1.7-.7 3-.7s1.9.7 3 .7c2.2 0 4.6-4 4.6-7.2-2.7-1-3.2-4.7-.5-6.1A4.4 4.4 0 0 0 15 8Z"/><path d="M15.5 3c.2 1.6-.8 3.4-2.8 3.8-.2-1.7 1-3.4 2.8-3.8Z"/></>,
  windows: <path d="M3 5.5 10.5 4v7H3V5.5ZM12 3.7 21 2v9h-9V3.7ZM3 13h7.5v7L3 18.5V13Zm9 0h9v9l-9-1.7V13Z"/>,
  linux: <><path d="M8 16c-2 1-3 3-3 5h14c0-2-1-4-3-5M8 11c0-5 1-8 4-8s4 3 4 8c0 4-1.8 7-4 7s-4-3-4-7Z"/><circle cx="10.5" cy="9" r=".6" fill="currentColor"/><circle cx="13.5" cy="9" r=".6" fill="currentColor"/><path d="M10 12c1.2.8 2.8.8 4 0"/></>,
  text: <><path d="M4 6V4h16v2M12 4v16M8 20h8"/></>, image: <><rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="9" cy="10" r="2"/><path d="m4 18 5-5 3 3 2-2 6 5"/></>,
  link: <><path d="m10 13 4-4"/><path d="M8 16H6a4 4 0 0 1 0-8h4M16 8h2a4 4 0 1 1 0 8h-4"/></>, files: <><path d="M4 3h10l4 4v14H4V3Z"/><path d="M14 3v5h5M8 13h8M8 17h6"/></>,
  code: <><path d="m8 9-4 3 4 3M16 9l4 3-4 3M14 5l-4 14"/></>, x: <path d="M6 6l12 12M18 6 6 18"/>,
  sparkles: <><path d="m12 3 1.2 3.8L17 8l-3.8 1.2L12 13l-1.2-3.8L7 8l3.8-1.2L12 3ZM5 14l.8 2.2L8 17l-2.2.8L5 20l-.8-2.2L2 17l2.2-.8L5 14ZM19 13l.7 1.8 1.8.7-1.8.7L19 18l-.7-1.8-1.8-.7 1.8-.7L19 13Z"/></>,
  plus: <path d="M12 5v14M5 12h14"/>,
  phone: <><rect x="6" y="2" width="12" height="20" rx="3"/><path d="M10 5h4M11 19h2"/></>,
  refresh: <><path d="M20 7v5h-5"/><path d="M4 17v-5h5"/><path d="M6.1 8a7 7 0 0 1 11.4-2.5L20 8M4 16l2.5 2.5A7 7 0 0 0 17.9 16"/></>,
};

export function Icon({ name, size = 18, ...props }: { name: IconName; size?: number } & SVGProps<SVGSVGElement>) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" {...props}>{paths[name]}</svg>;
}
