import { useMemo, useState } from "react";
import type { DesktopClient } from "../../ipc/client";
import type { UiSnapshot } from "../../model";
import { Icon } from "../../components/Icon";
import { EmptyState } from "../../components/EmptyState";
import { CurrentClipboard } from "./CurrentClipboard";
import { DeviceSlotCard } from "./DeviceSlotCard";
import "./clipboard.css";

export function ClipboardSwitcher({ snapshot, client, onError, compact = false }: { snapshot: UiSnapshot; client: DesktopClient; onError: (message: string) => void; compact?: boolean }) {
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState(snapshot.slots[0]?.id ?? "");
  const filtered = useMemo(() => snapshot.slots.filter((slot) => `${slot.deviceName} ${slot.groups.join(" ")} ${slot.representations.map((item) => item.label).join(" ")}`.toLowerCase().includes(query.trim().toLowerCase())), [query, snapshot.slots]);

  const execute = (action: Promise<unknown>) => { void action.catch((reason: unknown) => onError(reason instanceof Error ? reason.message : "操作失败")); };
  return <div className={`switcher ${compact ? "summary" : ""}`}>
    {!compact && <div className="switcher-toolbar">
      <label className="search-box"><Icon name="search" size={17} /><input className="search-input" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索设备、同步组或类型…" aria-label="搜索设备剪贴板" /></label>
      <button type="button" className="button pause-button" onClick={() => execute(client.setPause("publish", !snapshot.publishPaused))}><Icon name={snapshot.publishPaused ? "play" : "pause"} size={15} />{snapshot.publishPaused ? "恢复发布" : "暂停发布"}</button>
    </div>}
    {!compact && <CurrentClipboard snapshot={snapshot} />}
    {!compact && <div className="separator-label">设备剪贴板</div>}
    {filtered.length > 0 ? <div className="slot-list">
      {filtered.map((slot) => <DeviceSlotCard key={slot.id} slot={slot} selected={selectedId === slot.id} onSelect={() => setSelectedId(slot.id)} operation={snapshot.imports.find((item) => item.slotId === slot.id)} interactionDisabled={snapshot.platform === "android" && snapshot.activity !== "foreground_live"} onUse={() => execute(client.createImportIntent(slot.id, slot.revision))} onConfirm={(id) => execute(client.confirmImport(id))} onCancel={(id) => execute(client.cancelImport(id))} />)}
    </div> : <EmptyState icon={snapshot.slots.length === 0 ? "devices" : "search"} title={snapshot.slots.length === 0 ? "还没有可信设备" : "没有匹配的设备"} description={snapshot.slots.length === 0 ? "完成设备配对后，各设备的最新剪贴板会显示在这里。" : "尝试搜索设备名称、同步组或内容类型。"} />}
  </div>;
}
