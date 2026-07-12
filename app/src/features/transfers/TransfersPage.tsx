import { EmptyState } from "../../components/EmptyState";
import { Icon } from "../../components/Icon";

export function TransfersPage() {
  return <div className="page">
    <header className="page-header"><div><p className="page-eyebrow">普通文件投送</p><h1 className="page-title">传输中心</h1><p className="page-subtitle">普通文件传输与文件剪贴板保持分离；接收、冲突处理和恢复都在这里进行。</p></div><button type="button" className="button primary" disabled><Icon name="plus" size={16} />发送文件</button></header>
    <div className="card"><EmptyState icon="transfer" title="还没有文件传输" description="发送与接收记录会显示在这里。" /></div>
  </div>;
}
