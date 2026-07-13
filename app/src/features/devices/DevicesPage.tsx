import { useState } from "react";
import type { DesktopClient } from "../../ipc/client";
import type { UiSnapshot } from "../../model";
import { Icon, type IconName } from "../../components/Icon";
import { StatusBadge } from "../../components/StatusBadge";
import { EmptyState } from "../../components/EmptyState";

const platformIcon: Record<UiSnapshot["trustedDevices"][number]["platform"], IconName> = {
  macos: "apple",
  windows: "windows",
  linux: "linux",
  android: "phone",
  unknown: "devices",
};

export function DevicesPage({ snapshot, client, onError }: {
  snapshot: UiSnapshot;
  client: DesktopClient;
  onError: (message: string) => void;
}) {
  const [pairingWindowOpen, setPairingWindowOpen] = useState(false);
  const execute = async (operation: () => Promise<void>) => {
    try {
      await operation();
    } catch (reason) {
      onError(reason instanceof Error ? reason.message : "设备操作失败");
    }
  };

  const allowPairing = () => execute(async () => {
    await client.allowPairing();
    setPairingWindowOpen(true);
    window.setTimeout(() => setPairingWindowOpen(false), 120_000);
  });

  return <div className="page">
    <header className="page-header"><div><p className="page-eyebrow">可信设备</p><h1 className="page-title">设备</h1><p className="page-subtitle">发现只负责找到设备；双方核对同一验证码并确认后，才建立可信连接。</p></div><button className="button primary" type="button" onClick={allowPairing}><Icon name="shield" size={16} />{pairingWindowOpen ? "配对窗口已开放" : "允许其他设备配对"}</button></header>
    {!snapshot.daemonConnected && <div className="notice"><Icon name="alert" size={17} /><div><strong>暂时无法连接</strong><p>请重新启动 AirDrop 后再试。</p></div></div>}

    {snapshot.pendingPairings.map((pairing) => <section className="pairing-panel" key={pairing.pairingId}>
      <div className="pairing-panel-copy"><StatusBadge tone="warning" icon="shield">需要双方确认</StatusBadge><h2>{pairing.deviceName}</h2><p>请在两台设备上核对这组六位验证码。不同就立即拒绝，不要继续。</p></div>
      <div className="pairing-code" aria-label={`配对验证码 ${pairing.sas}`}>{pairing.sas.slice(0, 3)} <span>{pairing.sas.slice(3)}</span></div>
      <div className="pairing-actions"><button type="button" className="button" onClick={() => execute(() => client.confirmPairing(pairing.pairingId, false))}>不匹配</button><button type="button" className="button primary" disabled={pairing.status === "waiting_for_peer"} onClick={() => execute(() => client.confirmPairing(pairing.pairingId, true))}>{pairing.status === "waiting_for_peer" ? "等待对方确认" : "验证码一致"}</button></div>
    </section>)}

    <section className="page-section"><h2>附近设备</h2><div className="card list-card">
      {snapshot.nearbyDevices.length === 0 ? <EmptyState icon="devices" title="正在查找附近设备" description="同一局域网内运行 AirDrop 的设备会自动出现在这里。" /> : snapshot.nearbyDevices.map((device) => <div className="list-row" key={device.instanceId}>
        <div className="device-avatar"><Icon name={platformIcon[device.platform]} size={20} /><i className="online-dot" /></div>
        <div className="list-row-main"><strong>{device.deviceName}</strong><span>{device.platform} · {device.addresses.join("、") || "地址解析中"}</span></div>
        <StatusBadge tone={device.paired ? "success" : "info"} icon={device.paired ? "check" : "shield"}>{device.paired ? "已配对" : "可配对"}</StatusBadge>
        <button type="button" className="button" disabled={device.paired || device.addresses.length === 0} onClick={() => execute(() => client.beginPairing(device.instanceId))}>{device.paired ? "已可信" : "配对"}</button>
      </div>)}
    </div></section>
    <section className="page-section"><h2>已授权设备</h2><div className="card list-card">
      {snapshot.trustedDevices.length === 0 ? <EmptyState icon="devices" title="尚未配对设备" description="完成双方验证码确认后，可信设备会保存在本机。" /> : snapshot.trustedDevices.map((device) => <div className="list-row" key={device.deviceId}>
        <div className="device-avatar"><Icon name={platformIcon[device.platform]} size={20} /><i className={`online-dot ${device.online ? "" : "offline"}`} /></div>
        <div className="list-row-main"><strong>{device.deviceName}</strong><span>{device.platform} · 身份已固定 · {new Date(device.pairedAt).toLocaleDateString()}</span></div>
        <StatusBadge tone={device.online ? "success" : "neutral"} icon={device.online ? "check" : "clock"}>{device.online ? "安全连接中" : "离线"}</StatusBadge>
      </div>)}
    </div></section>
  </div>;
}
