import { useEffect, useState } from "react";
import type { DesktopClient } from "../../ipc/client";
import { EMPTY_TELEMETRY, formatBytes, formatRate, type TelemetrySnapshot, type UiSnapshot } from "../../model";
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

const formatDuration = (milliseconds: number): string => {
  if (milliseconds < 1_000) return `${milliseconds} ms`;
  if (milliseconds < 60_000) return `${(milliseconds / 1_000).toFixed(1)} 秒`;
  return `${Math.floor(milliseconds / 60_000)} 分 ${Math.round(milliseconds % 60_000 / 1_000)} 秒`;
};

const formatConnectionAge = (connectedAt: string | null): string => {
  if (!connectedAt) return "未连接";
  const elapsed = Math.max(0, Date.now() - new Date(connectedAt).getTime());
  return formatDuration(elapsed);
};

const transferStatusLabel = (status: TelemetrySnapshot["transfers"][number]["status"]): string => {
  if (status === "success") return "已确认";
  if (status === "sent") return "已发送但未确认";
  if (status === "failed") return "失败";
  return "同步中";
};

export function DevicesPage({ snapshot, telemetry = EMPTY_TELEMETRY, client, onError }: {
  snapshot: UiSnapshot;
  telemetry?: TelemetrySnapshot;
  client: DesktopClient;
  onError: (message: string) => void;
}) {
  const [pairingClock, setPairingClock] = useState(() => Date.now());
  const [revokingId, setRevokingId] = useState<string | null>(null);
  const [editingAliasId, setEditingAliasId] = useState<string | null>(null);
  const [aliasDraft, setAliasDraft] = useState("");
  const [savingAliasId, setSavingAliasId] = useState<string | null>(null);
  const [detailsId, setDetailsId] = useState<string | null>(null);
  const [copiedDiagnosticId, setCopiedDiagnosticId] = useState<string | null>(null);
  const pairingWindowOpen = snapshot.pairingAllowedUntil !== null
    && snapshot.pairingAllowedUntil * 1000 > pairingClock;

  useEffect(() => {
    if (snapshot.pairingAllowedUntil === null) return;
    const remaining = snapshot.pairingAllowedUntil * 1000 - Date.now();
    if (remaining <= 0) {
      setPairingClock(Date.now());
      return;
    }
    const timeout = window.setTimeout(() => setPairingClock(Date.now()), remaining + 50);
    return () => window.clearTimeout(timeout);
  }, [snapshot.pairingAllowedUntil]);
  const execute = async (operation: () => Promise<void>) => {
    try {
      await operation();
    } catch (reason) {
      onError(reason instanceof Error ? reason.message : "设备操作失败");
    }
  };

  const allowPairing = () => execute(() => client.allowPairing());
  const beginAliasEdit = (device: UiSnapshot["trustedDevices"][number]) => {
    setEditingAliasId(device.deviceId);
    setAliasDraft(device.localAlias ?? "");
  };
  const saveAlias = async (deviceId: string, localAlias: string | null) => {
    setSavingAliasId(deviceId);
    try {
      await client.setDeviceAlias(deviceId, localAlias);
      setEditingAliasId(null);
      setAliasDraft("");
    } catch (reason) {
      onError(reason instanceof Error ? reason.message : "设备备注名保存失败");
    } finally {
      setSavingAliasId(null);
    }
  };
  const copyDiagnostics = async (device: UiSnapshot["trustedDevices"][number]) => {
    const peer = telemetry.peers.find((item) => item.deviceId === device.deviceId);
    const transfers = telemetry.transfers.filter((item) => item.deviceId === device.deviceId).slice(0, 8);
    const report = [
      "AirDrop 设备诊断摘要",
      `生成时间：${new Date().toLocaleString()}`,
      `设备：${device.deviceName} (${device.platform})`,
      `设备 ID：${device.deviceId}`,
      `同步：${device.syncEnabled ? "启用" : "停用"}`,
      `连接：${peer?.connected ? "在线" : "离线"}`,
      `RTT：${peer?.rttMs == null ? "--" : `${peer.rttMs} ms`}`,
      `最近上传：${formatRate(peer?.recentUploadBps ?? 0)}`,
      `最近下载：${formatRate(peer?.recentDownloadBps ?? 0)}`,
      `丢包：${(peer?.lossPercent ?? 0).toFixed(2)}%`,
      `重连次数：${peer?.reconnectCount ?? 0}`,
      `异常断联：${peer?.unexpectedDisconnectCount ?? 0}`,
      `最近通信：${peer?.lastActivityAt ? new Date(peer.lastActivityAt).toLocaleString() : "无记录"}`,
      `最近断联：${peer?.lastDisconnectReason ?? "无记录"}${peer?.lastDisconnectCode ? ` (${peer.lastDisconnectCode})` : ""}`,
      `断联时间：${peer?.lastDisconnectedAt ? new Date(peer.lastDisconnectedAt).toLocaleString() : "无记录"}`,
      "",
      "最近传输：",
      ...(transfers.length ? transfers.map((transfer) => [
        new Date(transfer.startedAt).toLocaleString(),
        transfer.direction === "upload" ? "发送" : "接收",
        transfer.kind,
        formatBytes(transfer.totalBytes),
        formatDuration(transfer.durationMs),
        transfer.networkDurationMs === null ? "网络耗时未知" : `网络 ${formatDuration(transfer.networkDurationMs)}`,
        transfer.confirmationDurationMs === null ? "确认耗时未知" : `确认 ${formatDuration(transfer.confirmationDurationMs)}`,
        transferStatusLabel(transfer.status),
        transfer.message ?? "",
      ].join(" · ")) : ["无记录"]),
    ].join("\n");
    try {
      await client.copyDiagnosticReport(report);
      setCopiedDiagnosticId(device.deviceId);
      window.setTimeout(() => setCopiedDiagnosticId((current) => current === device.deviceId ? null : current), 1_500);
    } catch (reason) {
      onError(reason instanceof Error ? reason.message : typeof reason === "string" ? reason : "诊断摘要复制失败");
    }
  };

  return <div className="page">
    <header className="page-header"><div><p className="page-eyebrow">可信设备</p><h1 className="page-title">设备</h1><p className="page-subtitle">发现只负责找到设备；双方核对同一验证码并确认后，才建立可信连接。</p></div><button className="button primary" type="button" onClick={allowPairing}><Icon name="shield" size={16} />{pairingWindowOpen ? "配对窗口已开放" : "允许其他设备配对"}</button></header>
    {!snapshot.daemonConnected && <div className="notice"><Icon name="alert" size={17} /><div><strong>暂时无法连接</strong><p>请重新启动 AirDrop 后再试。</p></div></div>}

    {snapshot.pendingPairings.map((pairing) => <section className="pairing-panel" key={pairing.pairingId}>
      <div className="pairing-panel-copy"><StatusBadge tone={pairing.status === "peer_confirmed" ? "success" : "warning"} icon="shield">{pairing.status === "peer_confirmed" ? "对方已确认" : "需要双方确认"}</StatusBadge><h2>{pairing.deviceName}</h2><p>{pairing.status === "peer_confirmed" ? "对方已确认验证码一致，请在本机核对后完成配对。" : "请在两台设备上核对这组六位验证码。不同就立即拒绝，不要继续。"}</p></div>
      <div className="pairing-code" aria-label={`配对验证码 ${pairing.sas}`}>{pairing.sas.slice(0, 3)} <span>{pairing.sas.slice(3)}</span></div>
      <div className="pairing-actions"><button type="button" className="button" disabled={pairing.status === "waiting_for_peer" || pairing.status === "waiting_for_peer_complete"} onClick={() => execute(() => client.confirmPairing(pairing.pairingId, false))}>{pairing.status === "waiting_for_peer_complete" ? "双方已确认" : pairing.status === "waiting_for_peer" ? "本机已确认" : "不匹配"}</button><button type="button" className="button primary" disabled={pairing.status === "waiting_for_peer" || pairing.status === "waiting_for_peer_complete"} onClick={() => execute(() => client.confirmPairing(pairing.pairingId, true))}>{pairing.status === "waiting_for_peer_complete" ? "正在提交信任" : pairing.status === "waiting_for_peer" ? "等待对方确认" : pairing.status === "peer_confirmed" ? "确认并完成" : "验证码一致"}</button></div>
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
      {snapshot.trustedDevices.length === 0 ? <EmptyState icon="devices" title="尚未配对设备" description="完成双方验证码确认后，可信设备会保存在本机。" /> : snapshot.trustedDevices.map((device) => {
        const peer = telemetry.peers.find((item) => item.deviceId === device.deviceId);
        const latestTransfer = telemetry.transfers.find((item) => item.deviceId === device.deviceId);
        const detailsOpen = detailsId === device.deviceId;
        return <div className="device-observability" key={device.deviceId}>
          <div className="list-row">
            <div className="device-avatar"><Icon name={platformIcon[device.platform]} size={20} /><i className={`online-dot ${device.online ? "" : "offline"}`} /></div>
            <div className="list-row-main"><strong>{device.deviceName}</strong><span>{device.localAlias ? `对方名称：${device.advertisedName} · ` : ""}{device.platform} · 身份已固定 · {new Date(device.pairedAt).toLocaleDateString()}</span><span className="device-live-summary">{peer?.connected ? `${peer.rttMs ?? "--"} ms · ↑ ${formatRate(peer.recentUploadBps)} · ↓ ${formatRate(peer.recentDownloadBps)}` : peer?.lastDisconnectReason ? `最近断联：${peer.lastDisconnectReason}` : "等待连接指标"}</span>{editingAliasId === device.deviceId && <form className="device-alias-editor" onSubmit={(event) => { event.preventDefault(); void saveAlias(device.deviceId, aliasDraft.trim() || null); }}><input className="text-field" value={aliasDraft} maxLength={48} autoFocus aria-label={`为 ${device.advertisedName} 设置本地备注名`} placeholder={device.advertisedName} onChange={(event) => setAliasDraft(event.target.value)} /><button type="submit" className="button compact primary" disabled={savingAliasId === device.deviceId}>{savingAliasId === device.deviceId ? "保存中" : "保存"}</button><button type="button" className="button compact" onClick={() => setEditingAliasId(null)}>取消</button></form>}</div>
            <StatusBadge tone={!device.syncEnabled ? "warning" : device.online ? "success" : "neutral"} icon={!device.syncEnabled ? "pause" : device.online ? "check" : "clock"}>{!device.syncEnabled ? "同步已停用" : device.online ? "安全连接中" : "离线"}</StatusBadge>
            <div className="device-row-actions">
              <button type="button" className="button" aria-expanded={detailsOpen} onClick={() => setDetailsId(detailsOpen ? null : device.deviceId)}>连接详情</button>
              {device.localAlias && <button type="button" className="button" disabled={savingAliasId === device.deviceId} onClick={() => void saveAlias(device.deviceId, null)}>恢复原名</button>}
              <button type="button" className="button" onClick={() => beginAliasEdit(device)}>备注名</button>
              <button type="button" className="button" onClick={() => execute(() => client.setDeviceSyncEnabled(device.deviceId, !device.syncEnabled))}>{device.syncEnabled ? "停用同步" : "恢复同步"}</button>
              {revokingId === device.deviceId ? <><button type="button" className="button" onClick={() => setRevokingId(null)}>取消</button><button type="button" className="button danger" onClick={() => execute(async () => { await client.revokeDevice(device.deviceId); setRevokingId(null); })}>确认解除</button></> : <button type="button" className="icon-button" aria-label={`解除与 ${device.deviceName} 的配对`} title="解除配对" onClick={() => setRevokingId(device.deviceId)}><Icon name="x" size={16} /></button>}
            </div>
          </div>
          {detailsOpen && <div className="device-telemetry-panel">
            <div className="device-telemetry-grid">
              <span><small>往返延迟</small><strong>{peer?.rttMs == null ? "--" : `${peer.rttMs} ms`}</strong></span>
              <span><small>最近上行</small><strong>{formatRate(peer?.recentUploadBps ?? 0)}</strong></span>
              <span><small>最近下行</small><strong>{formatRate(peer?.recentDownloadBps ?? 0)}</strong></span>
              <span><small>丢包</small><strong>{(peer?.lossPercent ?? 0).toFixed(2)}%</strong></span>
              <span><small>连接时长</small><strong>{formatConnectionAge(peer?.connectedAt ?? null)}</strong></span>
              <span><small>重连次数</small><strong>{peer?.reconnectCount ?? 0}</strong></span>
              <span><small>异常断联</small><strong>{peer?.unexpectedDisconnectCount ?? 0}</strong></span>
            </div>
            <div className="device-telemetry-foot">
              <div><strong>最近同步</strong><span>{latestTransfer ? `${latestTransfer.direction === "upload" ? "发送" : "接收"} ${latestTransfer.kind} · ${formatBytes(latestTransfer.totalBytes)} · ${formatDuration(latestTransfer.durationMs)} · ${latestTransfer.message ?? transferStatusLabel(latestTransfer.status)}` : "暂无传输记录"}</span><small>累计 ↑ {formatBytes(peer?.totalUploadedBytes ?? 0)} · ↓ {formatBytes(peer?.totalDownloadedBytes ?? 0)}{peer?.lastActivityAt ? ` · 最近通信 ${new Date(peer.lastActivityAt).toLocaleTimeString()}` : ""}{peer?.lastDisconnectReason ? ` · 最近断联：${peer.lastDisconnectReason}${peer.lastDisconnectCode ? ` (${peer.lastDisconnectCode})` : ""}` : ""}{peer?.unexpectedDisconnectCount ? ` · 异常 ${peer.unexpectedDisconnectCount} 次` : ""}</small></div>
              <button type="button" className="button compact" onClick={() => void copyDiagnostics(device)}>{copiedDiagnosticId === device.deviceId ? "已复制" : "复制诊断摘要"}</button>
            </div>
          </div>}
        </div>;
      })}
    </div></section>
  </div>;
}
