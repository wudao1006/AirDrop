import { useState } from "react";
import type { DeviceSlot, ImportOperation } from "../../model";
import { formatBytes } from "../../model";
import { Icon, type IconName } from "../../components/Icon";
import { StatusBadge } from "../../components/StatusBadge";
import { RepresentationList } from "./RepresentationList";
import { ImportProgress } from "./ImportProgress";

const platformIcon: Record<DeviceSlot["platform"], IconName> = { macos: "apple", windows: "windows", linux: "linux" };

const statusCopy: Record<DeviceSlot["availability"], { label: string; tone: "success" | "warning" | "danger" | "info" | "neutral"; icon: IconName }> = {
  metadata_only: { label: "仅元数据", tone: "info", icon: "download" },
  partial: { label: "部分就绪", tone: "warning", icon: "clock" },
  ready: { label: "已就绪", tone: "success", icon: "check" },
  stale: { label: "离线可用", tone: "warning", icon: "clock" },
  expired: { label: "已过期", tone: "neutral", icon: "clock" },
  blocked: { label: "已阻止", tone: "danger", icon: "shield" },
  protocol_conflict: { label: "安全冲突", tone: "danger", icon: "alert" },
};

export function DeviceSlotCard({ slot, operation, selected, onSelect, onUse, onConfirm, onCancel, interactionDisabled = false }: {
  slot: DeviceSlot;
  operation?: ImportOperation;
  selected: boolean;
  onSelect: () => void;
  onUse: () => void;
  onConfirm: (importId: string) => void;
  onCancel: (importId: string) => void;
  interactionDisabled?: boolean;
}) {
  const [expanded, setExpanded] = useState(false);
  const status = statusCopy[slot.availability];
  const disabled = interactionDisabled || ["expired", "blocked", "protocol_conflict"].includes(slot.availability) || Boolean(operation && !["imported", "failed", "unavailable"].includes(operation.status));
  const actionLabel = slot.availability === "ready" || slot.availability === "stale" ? "使用" : slot.availability === "partial" ? "继续获取" : "获取并使用";
  return <article className={`slot-card ${selected ? "selected" : ""} ${slot.availability === "protocol_conflict" ? "conflict" : ""}`} onClick={onSelect} aria-label={`${slot.deviceName}，${status.label}`}>
    <div className="slot-main">
      <div className="device-avatar"><Icon name={platformIcon[slot.platform]} size={21} /><i className={`online-dot ${slot.online ? "" : "offline"}`} /></div>
      <div className="slot-content">
        <div className="slot-title-row"><h3 className="slot-title">{slot.deviceName}</h3>{slot.pinned && <StatusBadge tone="neutral">已固定</StatusBadge>}<StatusBadge tone={status.tone} icon={status.icon}>{status.label}</StatusBadge><span className="slot-age">{slot.online ? "在线" : "离线"} · {slot.ageLabel}</span></div>
        <p className="slot-preview">{slot.preview}</p>
        <div className="slot-meta"><span>{slot.representations.map((item) => item.label).join("、")}</span><span>·</span><span>{formatBytes(slot.size)}</span><span>·</span><span>#{slot.sequence}</span><span className="slot-groups">{slot.groups.map((group) => <span className="group-pill" key={group}>{group}</span>)}</span></div>
      </div>
      <div className="slot-actions">
        <button type="button" className="expand-button" aria-label={expanded ? "收起格式详情" : "展开格式详情"} aria-expanded={expanded} onClick={(event) => { event.stopPropagation(); setExpanded((value) => !value); }}><span className={expanded ? "expand-button open" : "expand-button"}><Icon name="chevron" size={16} /></span></button>
        <button type="button" className="button primary" disabled={disabled} onClick={(event) => { event.stopPropagation(); onUse(); }}>{actionLabel}</button>
      </div>
    </div>
    {slot.blockedReason && <div className="blocked-reason"><Icon name="alert" size={14} />{slot.blockedReason}</div>}
    {expanded && <RepresentationList representations={slot.representations} />}
    {operation && <ImportProgress operation={operation} onConfirm={() => onConfirm(operation.id)} onCancel={() => onCancel(operation.id)} />}
  </article>;
}
