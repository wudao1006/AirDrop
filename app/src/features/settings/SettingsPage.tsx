import { useEffect, useState } from "react";
import type { DesktopClient } from "../../ipc/client";
import type { AppSettings, UiSnapshot } from "../../model";
import { Toggle } from "../../components/Toggle";
import { Icon } from "../../components/Icon";
import { AppearanceSettings } from "./AppearanceSettings";
import { AppUpdater } from "./AppUpdater";

const shortcutLabels = (shortcut: string): string[] => shortcut.split("+").map((part) => {
  if (part.startsWith("Key")) return part.slice(3);
  if (part.startsWith("Digit")) return part.slice(5);
  if (part === "Super") return "Win";
  return part;
});

function ShortcutRecorder({ value, onSave, onError }: { value: string; onSave: (shortcut: string) => Promise<void>; onError: (message: string) => void }) {
  const [recording, setRecording] = useState(false);
  const [saving, setSaving] = useState(false);
  const capture = (event: React.KeyboardEvent<HTMLButtonElement>) => {
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") { setRecording(false); return; }
    if (["Control", "Alt", "Shift", "Meta"].includes(event.key) || event.repeat) return;
    const supportedCode = /^(Key[A-Z]|Digit[0-9]|F(?:[1-9]|1[0-2]))$/.test(event.code);
    if (!supportedCode || !(event.ctrlKey || event.altKey || event.shiftKey || event.metaKey)) {
      onError("快捷键需要至少一个修饰键，并使用字母、数字或 F1–F12");
      return;
    }
    const parts = [
      event.ctrlKey ? "Ctrl" : "",
      event.altKey ? "Alt" : "",
      event.shiftKey ? "Shift" : "",
      event.metaKey ? "Super" : "",
      event.code,
    ].filter(Boolean);
    setRecording(false);
    setSaving(true);
    void onSave(parts.join("+")).catch((reason: unknown) => {
      onError(reason instanceof Error ? reason.message : "快捷键保存失败");
    }).finally(() => setSaving(false));
  };
  return <button
    type="button"
    className={`shortcut-recorder ${recording ? "recording" : ""}`}
    aria-label="自定义全局快捷键"
    onClick={() => setRecording(true)}
    onKeyDown={capture}
    onBlur={() => setRecording(false)}
  >
    {recording ? <span className="shortcut-recording-copy">请按下新组合键…</span> : <span className="shortcut-keys">{shortcutLabels(value).map((part) => <kbd key={part}>{part}</kbd>)}</span>}
    <span className="shortcut-edit-copy">{saving ? "保存中" : "修改"}</span>
  </button>;
}

function DeviceNameEditor({ value, onSave, onError }: {
  value: string;
  onSave: (deviceName: string) => Promise<void>;
  onError: (message: string) => void;
}) {
  const [draft, setDraft] = useState(value);
  const [saving, setSaving] = useState(false);
  useEffect(() => setDraft(value), [value]);
  const changed = draft.trim() !== value;
  const save = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!changed || saving) return;
    setSaving(true);
    try {
      await onSave(draft);
    } catch (reason) {
      onError(reason instanceof Error ? reason.message : "本机名称保存失败");
    } finally {
      setSaving(false);
    }
  };
  return <form className="device-name-editor" onSubmit={save}>
    <label className="field-label" htmlFor="local-device-name">本机对外名称</label>
    <div className="device-name-editor-row">
      <input id="local-device-name" className="text-field" value={draft} maxLength={48} onChange={(event) => setDraft(event.target.value)} aria-describedby="local-device-name-help" />
      <button className="button primary" type="submit" disabled={!changed || !draft.trim() || saving}>{saving ? "保存中" : "保存名称"}</button>
    </div>
    <span id="local-device-name-help">此名称会广播给已连接设备；设备 ID 与密钥不会改变，也无需重新配对。</span>
  </form>;
}

export function SettingsPage({ snapshot, client, onError }: { snapshot: UiSnapshot; client: DesktopClient; onError: (message: string) => void }) {
  const update = (settings: Partial<AppSettings>) => { void client.updateSettings(settings).catch((reason: unknown) => onError(reason instanceof Error ? reason.message : "设置保存失败")); };
  return <div className="page">
    <header className="page-header"><div><p className="page-eyebrow">本地策略</p><h1 className="page-title">设置</h1><p className="page-subtitle">支持复制的类型会按 capability 自动出现，由你决定哪些类型允许发布、订阅和预取。</p></div></header>
    <section className="page-section device-profile-section"><h2>设备名称</h2><div className="card device-profile-card"><div className="device-profile-icon"><Icon name="devices" size={22} /></div><div className="device-profile-copy"><strong>让其他设备认出这台设备</strong><p>设置一个稳定、易辨认的名称。其他设备仍可在各自本地为你添加单向备注名。</p><DeviceNameEditor value={snapshot.localDeviceName} onSave={(deviceName) => client.setLocalDeviceName(deviceName)} onError={onError} /></div></div></section>
    <div className="grid-2">
      <section><div className="section-title"><h2>同步控制</h2></div><div className="card settings-card"><Toggle label="发布本机剪贴板槽位" description="只同步本机新复制的内容，不转发从其他设备取用的内容" checked={!snapshot.publishPaused} onChange={(checked) => void client.setPause("publish", !checked)} /><Toggle label="订阅远端设备槽位" description="实时更新设备卡片，但不会自动写入本机剪贴板" checked={!snapshot.subscribePaused} onChange={(checked) => void client.setPause("subscribe", !checked)} /></div></section>
      <section><div className="section-title"><h2>{snapshot.platform === "desktop" ? "快捷操作" : "运行模式"}</h2></div><div className="card settings-card">{snapshot.platform === "desktop" ? <div className="toggle-row"><div className="toggle-copy"><strong>唤起悬浮球</strong><span>优先打开悬浮球快捷菜单；悬浮球关闭时打开主窗口</span></div><ShortcutRecorder value={snapshot.settings.globalShortcut} onSave={(shortcut) => client.setGlobalShortcut(shortcut)} onError={onError} /></div> : <div className="toggle-row"><div className="toggle-copy"><strong>前台实时模式</strong><span>退到后台允许系统暂停，回到前台自动恢复</span></div><span className="tag">轻量模式</span></div>}</div></section>
    </div>
    {snapshot.platform === "desktop" && <AppUpdater />}
    <AppearanceSettings settings={snapshot.settings} platform={snapshot.platform} onUpdate={update} />
    {snapshot.platform === "android" && <div className="notice" style={{ marginTop: 18 }}><Icon name="phone" size={17} /><div><strong>Android 文本同步模式</strong><p>当前版本只发布和取用纯文本与 URL；图片、富文本和文件会通过能力协商阻止发送。</p></div></div>}
    <section className="page-section"><h2>允许的内容类型</h2><div className="card settings-card"><Toggle label="纯文本" description="text/plain · 默认允许" checked={snapshot.settings.allowText} onChange={(value) => update({ allowText: value })} />{snapshot.platform === "desktop" && <Toggle label="富文本与 HTML" description="保留格式，同时提供纯文本降级" checked={snapshot.settings.allowHtml} onChange={(value) => update({ allowHtml: value })} />}{snapshot.platform === "desktop" && <Toggle label="图片" description="图片正文可同步；缩略图由隐私设置单独控制" checked={snapshot.settings.allowImages} onChange={(value) => update({ allowImages: value })} />}<Toggle label="URL" description="独立于普通文本控制，不自动请求网页标题或图标" checked={snapshot.settings.allowUrls} onChange={(value) => update({ allowUrls: value })} />{snapshot.platform === "desktop" && <Toggle label="文件与目录" description="需要完整下载校验后才能取入本机" checked={snapshot.settings.allowFiles} onChange={(value) => update({ allowFiles: value })} />}{snapshot.platform === "desktop" && <Toggle label="应用私有格式" description="尚未启用安全的跨应用格式注册表" checked={false} onChange={() => undefined} disabled />}</div></section>
    <section className="page-section"><h2>预览与隐私</h2><div className="card settings-card"><Toggle label="显示截断文本预览" checked={snapshot.settings.previewText} onChange={(value) => update({ previewText: value })} />{snapshot.platform === "desktop" && <Toggle label="显示图片缩略图" checked={snapshot.settings.previewImages} onChange={(value) => update({ previewImages: value })} />}{snapshot.platform === "desktop" && <Toggle label="显示文件名" checked={snapshot.settings.previewFileNames} onChange={(value) => update({ previewFileNames: value })} />}</div></section>
    <div className="notice" style={{ marginTop: 18 }}><Icon name="shield" size={17} /><div><strong>{snapshot.cachePersistent ? "加密短期缓存已启用" : "当前仅使用内存缓存"}</strong><p>{snapshot.cachePersistent ? "远端文本正文使用系统凭据保护的 XChaCha20-Poly1305 缓存，24 小时后自动过期。" : "系统凭据存储不可用，因此剪贴板正文不会写入磁盘，重启后槽位正文需要重新同步。"}</p></div></div>
  </div>;
}
