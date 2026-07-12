import { Icon, type IconName } from "./Icon";

export function StatusBadge({ tone = "neutral", icon, children }: { tone?: "success" | "warning" | "danger" | "info" | "neutral"; icon?: IconName; children: React.ReactNode }) {
  return <span className={`status-badge ${tone}`}>{icon && <Icon name={icon} size={12} />}{children}</span>;
}
