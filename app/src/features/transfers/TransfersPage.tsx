import { EmptyState } from "../../components/EmptyState";
import { Icon, type IconName } from "../../components/Icon";
import { StatusBadge } from "../../components/StatusBadge";
import { EMPTY_TELEMETRY, formatBytes, formatRate, type TelemetrySnapshot, type TransferTelemetry, type UiSnapshot } from "../../model";

const kindLabel: Record<TransferTelemetry["kind"], string> = {
  text: "文本",
  url: "链接",
  html: "富文本",
  image: "图片",
  files: "文件",
};

const kindIcon: Record<TransferTelemetry["kind"], IconName> = {
  text: "text",
  url: "link",
  html: "code",
  image: "image",
  files: "files",
};

const statusLabel = (status: TransferTelemetry["status"]): string => {
  if (status === "success") return "已确认";
  if (status === "sent") return "已发送";
  if (status === "failed") return "失败";
  return "同步中";
};

const durationLabel = (milliseconds: number): string => milliseconds < 1_000
  ? `${milliseconds} ms`
  : `${(milliseconds / 1_000).toFixed(milliseconds < 10_000 ? 1 : 0)} 秒`;

const transferProgress = (transfer: TransferTelemetry): number => transfer.totalBytes === 0
  ? 0
  : Math.min(100, Math.round(transfer.transferredBytes / transfer.totalBytes * 100));

export function TransfersPage({ snapshot, telemetry = EMPTY_TELEMETRY }: {
  snapshot: UiSnapshot;
  telemetry?: TelemetrySnapshot;
}) {
  const active = telemetry.transfers.filter((transfer) => transfer.status === "active");
  const recent = telemetry.transfers.filter((transfer) => transfer.status !== "active");
  const peerName = (deviceId: string): string => snapshot.trustedDevices.find((device) => device.deviceId === deviceId)?.deviceName
    ?? snapshot.slots.find((slot) => slot.deviceId === deviceId)?.deviceName
    ?? "未知设备";
  const activeRate = active.reduce((total, transfer) => total + transfer.speedBps, 0);
  const successful = recent.filter((transfer) => transfer.status === "success").length;

  return <div className="page">
    <header className="page-header"><div><p className="page-eyebrow">同步可观测性</p><h1 className="page-title">传输中心</h1><p className="page-subtitle">查看设备之间的实时进度、实际传输速度和最近结果；历史仅保存传输元数据，不记录剪贴板正文。</p></div></header>
    <div className="grid-3 transfer-metrics">
      <div className="card metric-card"><span className="metric-icon accent"><Icon name="transfer" /></span><div><span>正在同步</span><strong>{active.length}</strong><small>图片与文件会显示实时进度</small></div></div>
      <div className="card metric-card"><span className="metric-icon success"><Icon name="refresh" /></span><div><span>当前吞吐</span><strong>{formatRate(activeRate)}</strong><small>所有活动传输的平滑实时速率</small></div></div>
      <div className="card metric-card"><span className="metric-icon warning"><Icon name="check" /></span><div><span>最近成功</span><strong>{successful}</strong><small>当前显示 {telemetry.transfers.length} 条，每台设备最多保留 10 条</small></div></div>
    </div>

    {active.length > 0 && <section className="page-section"><h2>正在同步</h2><div className="transfer-active-list">
      {active.map((transfer) => {
        const progress = transferProgress(transfer);
        const remaining = Math.max(0, transfer.totalBytes - transfer.transferredBytes);
        const eta = transfer.speedBps > 0 ? Math.ceil(remaining / transfer.speedBps) : null;
        return <article className="card transfer-card" key={transfer.attemptId}>
          <span className="transfer-kind-icon"><Icon name={kindIcon[transfer.kind]} /></span>
          <div className="transfer-copy"><div className="transfer-title"><strong>{transfer.direction === "upload" ? "发送到" : "接收自"} {peerName(transfer.deviceId)}</strong><span>{kindLabel[transfer.kind]} · {formatBytes(transfer.totalBytes)}</span></div><div className="transfer-progress"><span style={{ width: `${progress}%` }} /></div><small>{progress}% · {formatRate(transfer.speedBps)}{eta === null ? "" : ` · 预计剩余 ${eta} 秒`}</small></div>
          <StatusBadge tone="info" icon="refresh">同步中</StatusBadge>
        </article>;
      })}
    </div></section>}

    <section className="page-section"><h2>最近记录</h2><div className="card list-card transfer-history">
      {recent.length === 0 ? <EmptyState icon="transfer" title="还没有同步记录" description="设备之间发送或接收剪贴板内容后，速度与结果会显示在这里。" /> : recent.map((transfer) => <div className="list-row" key={transfer.attemptId}>
        <span className="transfer-history-icon"><Icon name={kindIcon[transfer.kind]} size={17} /></span>
        <div className="list-row-main"><strong>{transfer.direction === "upload" ? "发送到" : "接收自"} {peerName(transfer.deviceId)}</strong><span>{kindLabel[transfer.kind]} · {formatBytes(transfer.totalBytes)} · {durationLabel(transfer.durationMs)}{transfer.kind === "image" || transfer.kind === "files" ? ` · 平均 ${formatRate(transfer.averageBps)}` : ""}</span>{transfer.networkDurationMs !== null && <small>网络 {durationLabel(transfer.networkDurationMs)}{transfer.confirmationDurationMs !== null ? ` · 确认 ${durationLabel(transfer.confirmationDurationMs)}` : ""}{transfer.remoteProcessingMs !== null ? ` · 对端处理 ${durationLabel(transfer.remoteProcessingMs)}` : ""}</small>}{transfer.message && <small>{transfer.message}</small>}</div>
        <StatusBadge tone={transfer.status === "success" ? "success" : transfer.status === "sent" ? "info" : "danger"} icon={transfer.status === "success" ? "check" : transfer.status === "sent" ? "transfer" : "alert"}>{statusLabel(transfer.status)}</StatusBadge>
        <time>{new Date(transfer.completedAt ?? transfer.startedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}</time>
      </div>)}
    </div></section>
  </div>;
}
