import type { AppActivity } from "../model";
import { Icon, type IconName } from "./Icon";
import { StatusBadge } from "./StatusBadge";

const activityCopy: Record<AppActivity, { label: string; description: string; tone: "success" | "warning" | "info"; icon: IconName }> = {
  foreground_live: { label: "前台实时", description: "设备槽位正在实时更新", tone: "success", icon: "check" },
  reconnecting: { label: "正在恢复", description: "正在重连设备并获取最新状态", tone: "info", icon: "refresh" },
  suspended: { label: "后台暂停", description: "回到应用后会自动恢复最新状态", tone: "warning", icon: "pause" },
};

export function ActivityStatus({ activity, compact = false, lastSynchronizedAt }: { activity: AppActivity; compact?: boolean; lastSynchronizedAt?: string }) {
  const copy = activityCopy[activity];
  if (compact) return <StatusBadge tone={copy.tone} icon={copy.icon}>{copy.label}</StatusBadge>;
  return <div className={`activity-banner ${activity}`}><div className="activity-icon"><Icon name={copy.icon} size={18} /></div><div><strong>{copy.label}</strong><span>{copy.description}{lastSynchronizedAt ? ` · 上次同步 ${new Date(lastSynchronizedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}` : ""}</span></div></div>;
}
