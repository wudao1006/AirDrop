import { Icon, type IconName } from "./Icon";

export function EmptyState({ icon, title, description, action }: { icon: IconName; title: string; description: string; action?: React.ReactNode }) {
  return <div className="empty-state"><div className="empty-state-content"><div className="empty-icon"><Icon name={icon} size={25} /></div><h3>{title}</h3><p>{description}</p>{action && <div className="empty-state-action">{action}</div>}</div></div>;
}
