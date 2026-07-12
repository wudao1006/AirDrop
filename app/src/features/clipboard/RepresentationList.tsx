import type { ClipboardRepresentation } from "../../model";
import { formatBytes } from "../../model";
import { Icon, type IconName } from "../../components/Icon";
import { StatusBadge } from "../../components/StatusBadge";

const icons: Record<ClipboardRepresentation["kind"], IconName> = { text: "text", html: "code", image: "image", url: "link", files: "files", private: "code" };

export function RepresentationList({ representations }: { representations: ClipboardRepresentation[] }) {
  return <div className="representations" aria-label="剪贴板表示">
    {representations.map((item) => <div className="representation-row" key={item.id}>
      <Icon name={icons[item.kind]} size={15} /><span>{item.label}</span><small>{item.mime}</small>
      <span className="representation-size">{formatBytes(item.size)}</span>
      {item.status !== "ready" && <StatusBadge tone={item.status === "fetching" ? "info" : "danger"}>{item.status === "fetching" ? "待获取" : item.status === "blocked" ? "已阻止" : "不兼容"}</StatusBadge>}
    </div>)}
  </div>;
}
