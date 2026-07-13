import type { UiSnapshot } from "../../model";
import { Icon } from "../../components/Icon";
import { StatusBadge } from "../../components/StatusBadge";

export function CurrentClipboard({ snapshot }: { snapshot: UiSnapshot }) {
  const current = snapshot.currentClipboard;
  const hasFiles = Boolean(current.fileNames?.length);
  const hasImage = Boolean(current.imagePreview);
  return <section className="current-clipboard" aria-labelledby="current-clipboard-title">
    <div className="current-top">
      <div className="current-source">
        <div className="source-icon"><Icon name={current.source === "remote" ? "download" : "copy"} size={19} /></div>
        <div><strong id="current-clipboard-title">当前系统剪贴板</strong><span>{current.sourceLabel}</span></div>
      </div>
      <StatusBadge tone={current.source === "remote" ? "info" : "neutral"} icon={current.source === "remote" ? "download" : "monitor"}>{current.types.join("、") || "等待内容"}</StatusBadge>
    </div>
    <div className={`clipboard-preview ${hasImage ? "has-image" : ""} ${hasFiles ? "has-files" : ""}`}>
      {hasImage && <img className="clipboard-image-preview" src={current.imagePreview} alt={current.preview || "剪贴板图片"} />}
      {hasFiles ? <ul className="clipboard-file-list" aria-label="剪贴板文件">
        {current.fileNames?.map((name, index) => <li key={`${name}-${index}`}><Icon name="files" size={15} /><span>{name}</span></li>)}
      </ul> : !hasImage && <span className="clipboard-text-content">{current.preview}</span>}
      {hasImage && <span className="clipboard-image-meta">{current.preview}</span>}
    </div>
    <div className="last-published"><Icon name="shield" size={13} /><span>{snapshot.lastPublishedPreview}</span></div>
  </section>;
}
