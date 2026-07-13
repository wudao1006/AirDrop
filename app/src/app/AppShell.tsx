import type { PageId, UiSnapshot } from "../model";
import { Icon } from "../components/Icon";
import { ActivityStatus } from "../components/ActivityStatus";
import { DesktopTitlebar } from "../platform/desktop/DesktopTitlebar";
import { desktopNavigation } from "../platform/desktop/navigation";
import { androidNavigation } from "../platform/android/navigation";

export function AppShell({ page, setPage, snapshot, children }: { page: PageId; setPage: (page: PageId) => void; snapshot: UiSnapshot; children: React.ReactNode }) {
  const navigation = snapshot.platform === "android" ? androidNavigation : desktopNavigation;
  return <div className={`app platform-${snapshot.platform}`}>
    {snapshot.platform === "desktop" && <DesktopTitlebar />}
    <aside className="sidebar">
      <div className="brand">
        <div className="brand-mark"><Icon name="logo" size={20} /></div>
        <div className="brand-copy"><strong>AirDrop</strong></div>
      </div>
      <nav className="nav" aria-label="主导航">
        {navigation.map((item) => <button key={item.id} type="button" className={`nav-button ${page === item.id ? "active" : ""}`} aria-current={page === item.id ? "page" : undefined} onClick={() => setPage(item.id)}>
          <Icon name={item.icon} size={18} /><span>{item.label}</span>
          {item.id === "clipboard" && snapshot.imports.some((operation) => operation.status === "awaiting_confirmation") && <span className="nav-count">!</span>}
          {item.id === "devices" && snapshot.pendingPairings.length > 0 && <span className="nav-count">{snapshot.pendingPairings.length}</span>}
          {item.id === "groups" && snapshot.pendingGroupInvites.length > 0 && <span className="nav-count">{snapshot.pendingGroupInvites.length}</span>}
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
