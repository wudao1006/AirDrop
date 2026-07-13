import type { DesktopClient } from "../../ipc/client";
import type { AppSettings, UiSnapshot } from "../../model";
import { Toggle } from "../../components/Toggle";
import { Icon } from "../../components/Icon";
import { AppearanceSettings } from "./AppearanceSettings";
import { AppUpdater } from "./AppUpdater";

export function SettingsPage({ snapshot, client, onError }: { snapshot: UiSnapshot; client: DesktopClient; onError: (message: string) => void }) {
  const update = (settings: Partial<AppSettings>) => { void client.updateSettings(settings).catch((reason: unknown) => onError(reason instanceof Error ? reason.message : "设置保存失败")); };
  return <div className="page">
    <header className="page-header"><div><p className="page-eyebrow">本地策略</p><h1 className="page-title">设置</h1><p className="page-subtitle">支持复制的类型会按 capability 自动出现，由你决定哪些类型允许发布、订阅和预取。</p></div></header>
    <div className="grid-2">
      <section><div className="section-title"><h2>同步控制</h2></div><div className="card settings-card"><Toggle label="发布本机剪贴板槽位" description="只同步本机新复制的内容，不转发从其他设备取用的内容" checked={!snapshot.publishPaused} onChange={(checked) => void client.setPause("publish", !checked)} /><Toggle label="订阅远端设备槽位" description="实时更新设备卡片，但不会自动写入本机剪贴板" checked={!snapshot.subscribePaused} onChange={(checked) => void client.setPause("subscribe", !checked)} /></div></section>
      <section><div className="section-title"><h2>{snapshot.platform === "desktop" ? "快捷操作" : "运行模式"}</h2></div><div className="card settings-card">{snapshot.platform === "desktop" ? <div className="toggle-row"><div className="toggle-copy"><strong>全局快捷键</strong><span>打开 Clipboard Switcher</span></div><span className="tag">⌘ ⇧ V</span></div> : <div className="toggle-row"><div className="toggle-copy"><strong>前台实时模式</strong><span>退到后台允许系统暂停，回到前台自动恢复</span></div><span className="tag">轻量模式</span></div>}</div></section>
    </div>
    {snapshot.platform === "desktop" && <AppUpdater />}
    <AppearanceSettings settings={snapshot.settings} platform={snapshot.platform} onUpdate={update} />
    <section className="page-section"><h2>允许的内容类型</h2><div className="card settings-card"><Toggle label="纯文本" description="text/plain · 默认允许" checked={snapshot.settings.allowText} onChange={(value) => update({ allowText: value })} /><Toggle label="富文本与 HTML" description="保留格式，同时提供纯文本降级" checked={snapshot.settings.allowHtml} onChange={(value) => update({ allowHtml: value })} /><Toggle label="图片" description="图片正文可同步；缩略图由隐私设置单独控制" checked={snapshot.settings.allowImages} onChange={(value) => update({ allowImages: value })} /><Toggle label="URL" description="独立于普通文本控制，不自动请求网页标题或图标" checked={snapshot.settings.allowUrls} onChange={(value) => update({ allowUrls: value })} /><Toggle label="文件与目录" description="需要完整下载校验后才能取入本机" checked={snapshot.settings.allowFiles} onChange={(value) => update({ allowFiles: value })} /><Toggle label="应用私有格式" description="尚未启用安全的跨应用格式注册表" checked={false} onChange={() => undefined} disabled /></div></section>
    <section className="page-section"><h2>预览与隐私</h2><div className="card settings-card"><Toggle label="显示截断文本预览" checked={snapshot.settings.previewText} onChange={(value) => update({ previewText: value })} /><Toggle label="显示图片缩略图" checked={snapshot.settings.previewImages} onChange={(value) => update({ previewImages: value })} /><Toggle label="显示文件名" checked={snapshot.settings.previewFileNames} onChange={(value) => update({ previewFileNames: value })} /></div></section>
    <div className="notice" style={{ marginTop: 18 }}><Icon name="shield" size={17} /><div><strong>{snapshot.cachePersistent ? "加密短期缓存已启用" : "当前仅使用内存缓存"}</strong><p>{snapshot.cachePersistent ? "远端文本正文使用系统凭据保护的 XChaCha20-Poly1305 缓存，24 小时后自动过期。" : "系统凭据存储不可用，因此剪贴板正文不会写入磁盘，重启后槽位正文需要重新同步。"}</p></div></div>
  </div>;
}
