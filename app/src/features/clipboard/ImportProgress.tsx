import type { ImportOperation } from "../../model";
import { Icon } from "../../components/Icon";
import { StatusBadge } from "../../components/StatusBadge";

export function ImportProgress({ operation, onConfirm, onCancel }: { operation: ImportOperation; onConfirm: () => void; onCancel: () => void }) {
  const awaiting = operation.status === "awaiting_confirmation";
  const imported = operation.status === "imported";
  return <div className="import-panel" aria-live="polite">
    <div className="import-top"><div><strong>{operation.sourceSummary}</strong><span>{operation.message}</span></div>
      <StatusBadge tone={imported ? "success" : awaiting ? "warning" : operation.status.includes("failed") || operation.status === "unavailable" ? "danger" : "info"} icon={imported ? "check" : awaiting ? "clock" : "download"}>
        {imported ? "已导入" : awaiting ? "等待确认" : operation.status === "fetching" ? `${operation.progress}%` : operation.status === "committing" ? "写入中" : "不可用"}
      </StatusBadge>
    </div>
    {operation.status === "fetching" && <div className="progress-track"><div className="progress-bar" style={{ width: `${operation.progress}%` }} /></div>}
    <div className="import-actions">
      {awaiting && <button type="button" className="button primary" onClick={onConfirm}><Icon name="download" size={15} />使用已就绪内容</button>}
      {!imported && operation.status !== "committing" && <button type="button" className="button ghost" onClick={onCancel}>取消</button>}
    </div>
  </div>;
}
