import type { UiSnapshot } from "../../model";
import { formatBytes } from "../../model";
import { Icon, type IconName } from "../../components/Icon";
import { StatusBadge } from "../../components/StatusBadge";
import { EmptyState } from "../../components/EmptyState";

const platformIcon: Record<UiSnapshot["slots"][number]["platform"], IconName> = { macos: "apple", windows: "windows", linux: "linux" };

export function DevicesPage({ snapshot }: { snapshot: UiSnapshot }) {
  return <div className="page">
    <header className="page-header"><div><p className="page-eyebrow">可信设备</p><h1 className="page-title">设备</h1><p className="page-subtitle">管理已验证身份、同步组与设备级剪贴板策略。在线状态不会被当作内容可用性的替代判断。</p></div><button className="button primary" type="button" disabled><Icon name="plus" size={16} />配对设备</button></header>
    {!snapshot.daemonConnected && <div className="notice"><Icon name="alert" size={17} /><div><strong>暂时无法连接</strong><p>请重新启动 AirDrop 后再试。</p></div></div>}
    <section className="page-section"><h2>已授权设备</h2><div className="card list-card">
      {snapshot.slots.length === 0 ? <EmptyState icon="devices" title="尚未配对设备" description="附近设备完成身份确认后会出现在这里。" /> : snapshot.slots.map((slot) => <div className="list-row" key={slot.deviceId}>
        <div className="device-avatar"><Icon name={platformIcon[slot.platform]} size={20} /><i className={`online-dot ${slot.online ? "" : "offline"}`} /></div>
        <div className="list-row-main"><strong>{slot.deviceName}</strong><span>{slot.groups.join(" · ")} · 最新槽位 {formatBytes(slot.size)} · sequence {slot.sequence}</span></div>
        <StatusBadge tone={slot.online ? "success" : "neutral"} icon={slot.online ? "check" : "clock"}>{slot.online ? "在线" : "离线"}</StatusBadge>
        <button type="button" className="icon-button" aria-label={`管理 ${slot.deviceName}`} disabled><Icon name="settings" size={16} /></button>
      </div>)}
    </div></section>
  </div>;
}
