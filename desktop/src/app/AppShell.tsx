import type { PageId, UiSnapshot } from "../model";
import { Icon, type IconName } from "../components/Icon";
import { ActivityStatus } from "../components/ActivityStatus";
import { getCurrentWindow } from "@tauri-apps/api/window";

const desktopNavigation: Array<{ id: PageId; label: string; icon: IconName }> = [
  { id: "home", label: "首页", icon: "home" },
  { id: "clipboard", label: "剪贴板", icon: "clipboard" },
  { id: "devices", label: "设备", icon: "devices" },
  { id: "groups", label: "同步组", icon: "groups" },
  { id: "transfers", label: "传输", icon: "transfer" },
  { id: "settings", label: "设置", icon: "settings" },
];

const androidNavigation = desktopNavigation.filter((item) => ["home", "clipboard", "devices", "settings"].includes(item.id));

function WindowTitlebar() {
  const run = (action: (appWindow: ReturnType<typeof getCurrentWindow>) => Promise<void>) => void action(getCurrentWindow()).catch(() => undefined);
  const keepControlInteractive = (event: React.PointerEvent<HTMLButtonElement>) => event.stopPropagation();

  return <header className="window-titlebar">
    <div className="window-titlebar-drag" data-tauri-drag-region />
    <div className="window-titlebar-brand" data-tauri-drag-region><i /></div>
    <div className="window-titlebar-title" data-tauri-drag-region>AirDrop</div>
    <div className="window-controls">
      <button type="button" aria-label="最小化" title="最小化" onPointerDown={keepControlInteractive} onClick={() => run((appWindow) => appWindow.minimize())}><span className="control-minimize" /></button>
      <button type="button" aria-label="最大化" title="最大化" onPointerDown={keepControlInteractive} onClick={() => run((appWindow) => appWindow.toggleMaximize())}><span className="control-maximize" /></button>
      <button type="button" className="control-close" aria-label="关闭" title="关闭" onPointerDown={keepControlInteractive} onClick={() => run((appWindow) => appWindow.close())}><span /></button>
    </div>
  </header>;
}

export function AppShell({ page, setPage, snapshot, children }: { page: PageId; setPage: (page: PageId) => void; snapshot: UiSnapshot; children: React.ReactNode }) {
  const attentionCount = snapshot.slots.filter((slot) => slot.availability === "blocked" || slot.availability === "protocol_conflict").length;
  const navigation = snapshot.platform === "android" ? androidNavigation : desktopNavigation;
  return <div className={`app platform-${snapshot.platform}`}>
    {snapshot.platform === "desktop" && <WindowTitlebar />}
    <aside className="sidebar">
      <div className="brand">
        <div className="brand-mark"><Icon name="logo" size={20} /></div>
        <div className="brand-copy"><strong>AirDrop</strong></div>
      </div>
      <nav className="nav" aria-label="主导航">
        {navigation.map((item) => <button key={item.id} type="button" className={`nav-button ${page === item.id ? "active" : ""}`} aria-current={page === item.id ? "page" : undefined} onClick={() => setPage(item.id)}>
          <Icon name={item.icon} size={18} /><span>{item.label}</span>
          {item.id === "clipboard" && snapshot.imports.some((operation) => operation.status === "awaiting_confirmation") && <span className="nav-count">!</span>}
          {item.id === "devices" && attentionCount > 0 && <span className="nav-count">{attentionCount}</span>}
        </button>)}
      </nav>
      <div className="sidebar-footer">
        <div className="connection-line"><i className={`dot ${snapshot.daemonConnected ? "" : "offline"}`} /><span>{snapshot.daemonConnected ? (snapshot.slots.some((slot) => slot.online) ? `${snapshot.slots.filter((slot) => slot.online).length} 台设备在线` : "等待设备") : "连接不可用"}</span></div>
      </div>
    </aside>
    <main className="main">
      {snapshot.platform === "android" && <div className="mobile-status-bar"><span><Icon name="phone" size={15} /> Android</span><ActivityStatus activity={snapshot.activity} compact /></div>}
      {children}
    </main>
  </div>;
}
