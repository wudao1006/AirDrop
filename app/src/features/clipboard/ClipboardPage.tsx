import type { DesktopClient } from "../../ipc/client";
import type { UiSnapshot } from "../../model";
import { Icon } from "../../components/Icon";
import { ClipboardSwitcher } from "./ClipboardSwitcher";

export function ClipboardPage({ snapshot, client, onError }: { snapshot: UiSnapshot; client: DesktopClient; onError: (message: string) => void }) {
  return <div className="page">
    <header className="page-header compact"><div><h1 className="page-title">剪贴板</h1><p className="page-subtitle">选择设备后才会写入当前系统剪贴板。</p></div>{snapshot.platform === "desktop" && <span className="shortcut-hint"><Icon name="sparkles" size={14} /><kbd>⌘ ⇧ V</kbd></span>}</header>
    <div className={`clipboard-page-grid ${snapshot.platform === "android" ? "android-switcher" : ""}`}>
      <ClipboardSwitcher snapshot={snapshot} client={client} onError={onError} />
    </div>
  </div>;
}
