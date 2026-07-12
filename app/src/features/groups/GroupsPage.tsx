import type { UiSnapshot } from "../../model";
import { Icon } from "../../components/Icon";
import { StatusBadge } from "../../components/StatusBadge";
import { EmptyState } from "../../components/EmptyState";

export function GroupsPage({ snapshot }: { snapshot: UiSnapshot }) {
  const groups = Array.from(new Set(snapshot.slots.flatMap((slot) => slot.groups))).map((name) => ({ name, members: snapshot.slots.filter((slot) => slot.groups.includes(name)) }));
  return <div className="page">
    <header className="page-header"><div><p className="page-eyebrow">权限边界</p><h1 className="page-title">同步组</h1><p className="page-subtitle">每个组独立控制成员方向、可同步类型、预取方式与缓存期限。UI 聚合不会扩大任何组的授权。</p></div><button type="button" className="button primary" disabled><Icon name="plus" size={16} />新建同步组</button></header>
    {groups.length === 0 ? <div className="card"><EmptyState icon="groups" title="还没有同步组" description="先完成可信设备配对，再创建决定发布、订阅和内容类型策略的同步组。" /></div> : <div className="grid-2">{groups.map((group) => <article className="card group-card" key={group.name}><div className="group-card-head"><div><h3>{group.name}</h3><p>{group.members.length + 1} 个成员 · 双向同步</p></div><StatusBadge tone="success" icon="shield">已验证</StatusBadge></div><div className="avatar-stack"><div className="mini-avatar">本机</div>{group.members.map((member) => <div className="mini-avatar" title={member.deviceName} key={member.deviceId}>{member.deviceName.slice(0, 1)}</div>)}</div><div className="policy-row"><span className="tag">文本</span><span className="tag">HTML</span><span className="tag">图片</span><span className="tag">URL</span><span className="tag">小内容预取</span></div></article>)}</div>}
    {!snapshot.daemonConnected && <div className="notice" style={{ marginTop: 18 }}><Icon name="alert" size={17} /><div><strong>同步组暂不可用</strong><p>请重新启动 AirDrop 后再试。</p></div></div>}
  </div>;
}
