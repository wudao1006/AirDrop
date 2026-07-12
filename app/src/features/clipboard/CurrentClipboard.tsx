import type { UiSnapshot } from "../../model";
import { Icon } from "../../components/Icon";
import { StatusBadge } from "../../components/StatusBadge";

export function CurrentClipboard({ snapshot }: { snapshot: UiSnapshot }) {
  const current = snapshot.currentClipboard;
  return <section className="current-clipboard" aria-labelledby="current-clipboard-title">
    <div className="current-top">
      <div className="current-source">
        <div className="source-icon"><Icon name={current.source === "remote" ? "download" : "copy"} size={19} /></div>
        <div><strong id="current-clipboard-title">当前系统剪贴板</strong><span>{current.sourceLabel}</span></div>
      </div>
      <StatusBadge tone={current.source === "remote" ? "info" : "neutral"} icon={current.source === "remote" ? "download" : "monitor"}>{current.types.join("、") || "等待内容"}</StatusBadge>
    </div>
    <p className="clipboard-preview">{current.preview}</p>
    <div className="last-published"><Icon name="shield" size={13} /><span>{snapshot.lastPublishedPreview}</span></div>
  </section>;
}
