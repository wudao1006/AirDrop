import { Icon, type IconName } from "./Icon";

export function EmptyState({ icon, title, description, action }: { icon: IconName; title: string; description: string; action?: React.ReactNode }) {
  return <div className="empty-state"><div><div className="empty-icon"><Icon name={icon} size={25} /></div><h3>{title}</h3><p>{description}</p>{action && <div style={{ marginTop: 16 }}>{action}</div>}</div></div>;
}
