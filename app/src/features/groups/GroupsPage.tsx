import { useState } from "react";
import type { DesktopClient } from "../../ipc/client";
import type { UiSnapshot } from "../../model";
import { Icon } from "../../components/Icon";
import { StatusBadge } from "../../components/StatusBadge";
import { EmptyState } from "../../components/EmptyState";

export function GroupsPage({ snapshot, client, onError }: {
  snapshot: UiSnapshot;
  client: DesktopClient;
  onError: (message: string) => void;
}) {
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [policy, setPolicy] = useState({ allowText: true, allowImages: true, allowHtml: false, allowFiles: false });
  const execute = async (operation: () => Promise<void>) => {
    try { await operation(); } catch (reason) { onError(reason instanceof Error ? reason.message : "同步组操作失败"); }
  };
  const toggleDevice = (deviceId: string) => setSelected((current) => {
    const next = new Set(current);
    if (next.has(deviceId)) next.delete(deviceId); else next.add(deviceId);
    return next;
  });
  const submit = () => execute(async () => {
    await client.createSyncGroup({ name: name.trim(), memberDeviceIds: [...selected], ...policy });
    setCreating(false);
    setName("");
    setSelected(new Set());
  });

  return <div className="page">
    <header className="page-header"><div><p className="page-eyebrow">权限边界</p><h1 className="page-title">同步组</h1><p className="page-subtitle">只有加入同一组且方向、内容策略同时允许的设备，才会交换剪贴板槽位。</p></div><button type="button" className="button primary" disabled={snapshot.trustedDevices.length === 0} onClick={() => setCreating((value) => !value)}><Icon name="plus" size={16} />{creating ? "收起" : "新建同步组"}</button></header>

    {creating && <section className="card group-create-panel">
      <div className="group-create-head"><div><h2>新建同步组</h2><p>邀请只会发送给已完成验证码配对的设备，成员必须明确接受。</p></div><StatusBadge tone="info" icon="shield">Owner：本机</StatusBadge></div>
      <label className="field-label">组名称<input className="text-field" value={name} maxLength={64} onChange={(event) => setName(event.target.value)} placeholder="例如：个人设备" /></label>
      <div className="group-create-grid"><div><strong className="field-title">选择设备</strong><div className="group-device-list">{snapshot.trustedDevices.map((device) => <label className={`group-device-option ${selected.has(device.deviceId) ? "selected" : ""}`} key={device.deviceId}><input type="checkbox" checked={selected.has(device.deviceId)} disabled={!device.syncEnabled} onChange={() => toggleDevice(device.deviceId)} /><span><b>{device.deviceName}</b><small>{device.platform} · {device.syncEnabled ? device.online ? "在线" : "离线，可稍后发送邀请" : "同步已停用"}</small></span></label>)}</div></div>
      <div><strong className="field-title">允许的内容</strong><div className="group-policy-list">{([['allowText', '纯文本 / URL'], ['allowImages', '图片'], ['allowHtml', 'HTML / RTF'], ['allowFiles', '文件剪贴板']] as const).map(([key, label]) => <label key={key}><input type="checkbox" checked={policy[key]} onChange={(event) => setPolicy((current) => ({ ...current, [key]: event.target.checked }))} />{label}</label>)}</div></div></div>
      <div className="group-create-actions"><button type="button" className="button" onClick={() => setCreating(false)}>取消</button><button type="button" className="button primary" disabled={!name.trim() || selected.size === 0} onClick={submit}>创建并发送邀请</button></div>
    </section>}

    {snapshot.pendingGroupInvites.map((invite) => <section className="pairing-panel" key={invite.inviteId}><div className="pairing-panel-copy"><StatusBadge tone="warning" icon="groups">同步组邀请</StatusBadge><h2>{invite.groupName}</h2><p>{invite.ownerName} 邀请本机加入。接受后，组策略允许的槽位才会自动同步。</p></div><div className="pairing-actions"><button type="button" className="button" onClick={() => execute(() => client.confirmGroupInvite(invite.inviteId, false))}>拒绝</button><button type="button" className="button primary" onClick={() => execute(() => client.confirmGroupInvite(invite.inviteId, true))}>接受邀请</button></div></section>)}

    {snapshot.syncGroups.length === 0 ? <div className="card"><EmptyState icon="groups" title="还没有同步组" description="先完成可信设备配对，再选择设备创建同步组。直接配对本身不会授予剪贴板同步权限。" /></div> : <div className="grid-2">{snapshot.syncGroups.map((group) => {
      const active = group.members.filter((member) => member.state === "active");
      const invited = group.members.filter((member) => member.state === "invited");
      const updatePolicy = (key: "allowText" | "allowImages" | "allowHtml" | "allowFiles", value: boolean) => client.updateGroupPolicy({ groupId: group.groupId, allowText: group.policy.allowText, allowImages: group.policy.allowImages, allowHtml: group.policy.allowHtml, allowFiles: group.policy.allowFiles, [key]: value });
      return <article className="card group-card" key={group.groupId}><div className="group-card-head"><div><h3>{group.name}</h3><p>{active.length} 个已授权成员{invited.length ? ` · ${invited.length} 个等待接受` : ""}</p></div><div className="group-card-status"><StatusBadge tone="success" icon="shield">签名清单 #{group.revision}</StatusBadge>{group.isOwner ? <button type="button" className="button danger compact" onClick={() => execute(() => client.deleteSyncGroup(group.groupId))}>结束同步组</button> : <button type="button" className="button danger compact" onClick={() => execute(() => client.leaveSyncGroup(group.groupId))}>退出组</button>}</div></div><div className="avatar-stack">{group.members.map((member) => <div className={`mini-avatar ${member.state !== "active" ? "pending" : ""}`} title={`${member.deviceName} · ${member.state}`} key={member.deviceId}>{member.deviceName.slice(0, 1)}</div>)}</div><div className="policy-row">{group.policy.allowText && <span className="tag">文本</span>}{group.policy.allowImages && <span className="tag">图片</span>}{group.policy.allowHtml && <span className="tag">HTML</span>}{group.policy.allowFiles && <span className="tag">文件</span>}<span className="tag">24 小时 TTL</span></div>{group.isOwner && <div className="group-policy-editor">{([['allowText', '文本'], ['allowImages', '图片'], ['allowHtml', 'HTML'], ['allowFiles', '文件']] as const).map(([key, label]) => <label key={key}><input type="checkbox" checked={group.policy[key]} onChange={(event) => execute(() => updatePolicy(key, event.target.checked))} />{label}</label>)}</div>}{group.isOwner && <div className="group-member-policy">{group.members.map((member) => <div className="group-member-row" key={member.deviceId}><span><b>{member.deviceName}</b><small>{member.state === "active" ? "已授权" : member.state === "invited" ? "等待接受邀请" : "已移除"}</small></span>{member.state === "active" && <select value={member.direction} aria-label={`${member.deviceName} 同步方向`} onChange={(event) => execute(() => client.setGroupMemberDirection(group.groupId, member.deviceId, event.target.value as "disabled" | "send_only" | "receive_only" | "bidirectional"))}><option value="bidirectional">双向</option><option value="send_only">仅发布</option><option value="receive_only">仅订阅</option><option value="disabled">禁用</option></select>}{member.deviceId !== group.ownerDeviceId && member.state !== "removed" && <button type="button" className="icon-button" title="移出同步组" aria-label={`移出 ${member.deviceName}`} onClick={() => execute(() => client.removeGroupMember(group.groupId, member.deviceId))}><Icon name="x" size={14} /></button>}</div>)}</div>}</article>;
    })}</div>}
    {!snapshot.daemonConnected && <div className="notice" style={{ marginTop: 18 }}><Icon name="alert" size={17} /><div><strong>同步组暂不可用</strong><p>请重新启动 AirDrop 后再试。</p></div></div>}
  </div>;
}
