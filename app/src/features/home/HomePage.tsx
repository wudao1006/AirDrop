import type { DesktopClient } from "../../ipc/client";
import type { UiSnapshot } from "../../model";
import { Icon } from "../../components/Icon";
import { CurrentClipboard } from "../clipboard/CurrentClipboard";
import { ClipboardSwitcher } from "../clipboard/ClipboardSwitcher";
import { ActivityStatus } from "../../components/ActivityStatus";

export function HomePage({ snapshot, client, onError, openClipboard }: { snapshot: UiSnapshot; client: DesktopClient; onError: (message: string) => void; openClipboard: () => void }) {
  return <div className="page">
    <header className="page-header"><div><h1 className="page-title">概览</h1><p className="page-subtitle">本机复制自动进入独立槽位，远端内容按需取用，不覆盖当前剪贴板。</p></div><div className="header-actions"><button type="button" className="button" disabled={snapshot.platform === "android" && snapshot.activity !== "foreground_live"} onClick={() => void client.publishCurrentClipboard().catch((reason: unknown) => onError(reason instanceof Error ? reason.message : "读取失败"))}><Icon name="copy" size={16} />{snapshot.clipboardCapability.canReadText ? "刷新本机剪贴板" : "重新尝试读取"}</button><button type="button" className="button primary" onClick={openClipboard}><Icon name="clipboard" size={16} />打开剪贴板</button></div></header>
    {snapshot.platform === "android" && <ActivityStatus activity={snapshot.activity} lastSynchronizedAt={snapshot.lastSynchronizedAt} />}
    {snapshot.platform === "android" && !snapshot.clipboardCapability.canReadText && <div className="notice" style={{ marginTop: 12 }}><Icon name="shield" size={17} /><div><strong>剪贴板读取受限</strong><p>{snapshot.clipboardCapability.limitation ?? "系统当前不允许读取文本剪贴板，仍可选择远端设备内容。"}</p></div></div>}
    <div className="section-title"><h2>当前剪贴板</h2><span>{snapshot.currentClipboard.sourceLabel}</span></div>
    <div className="surface-panel"><CurrentClipboard snapshot={snapshot} /></div>
    <div className="section-title"><h2>设备最新槽位</h2><button type="button" className="button ghost" onClick={openClipboard}>查看全部 <Icon name="chevron" size={14} /></button></div>
    <ClipboardSwitcher snapshot={{ ...snapshot, slots: snapshot.slots.slice(0, 2) }} client={client} onError={onError} compact />
  </div>;
}
