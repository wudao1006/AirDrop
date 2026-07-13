import { useEffect, useMemo, useRef, useState } from "react";
import { Icon } from "../../components/Icon";
import { createUpdaterAdapter, type AvailableUpdate, type UpdateProgress, type UpdaterAdapter } from "./updater-adapter";

type UpdatePhase = "idle" | "checking" | "available" | "current" | "downloading" | "installing" | "error";

const formatBytes = (bytes: number): string => {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
};

const statusCopy = (phase: UpdatePhase, version?: string): string => {
  switch (phase) {
    case "checking": return "正在安全检查新版本…";
    case "available": return `发现新版本 ${version ?? ""}`.trim();
    case "current": return "当前已经是最新版本";
    case "downloading": return "正在下载并校验更新包…";
    case "installing": return "更新已下载，正在安装并重启…";
    case "error": return "更新检查或安装失败";
    default: return "可手动检查更新；安装包会先验证签名，再替换当前版本。";
  }
};

export function AppUpdater({ adapter: suppliedAdapter }: { adapter?: UpdaterAdapter }) {
  const adapter = useMemo(() => suppliedAdapter ?? createUpdaterAdapter(), [suppliedAdapter]);
  const [currentVersion, setCurrentVersion] = useState("读取中…");
  const [phase, setPhase] = useState<UpdatePhase>("idle");
  const [availableVersion, setAvailableVersion] = useState<string>();
  const [notes, setNotes] = useState<string>();
  const [progress, setProgress] = useState<UpdateProgress>({ downloaded: 0 });
  const [error, setError] = useState<string>();
  const updateRef = useRef<AvailableUpdate | null>(null);
  const installingRef = useRef(false);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    void adapter.getCurrentVersion().then((version) => {
      if (mountedRef.current) setCurrentVersion(version);
    }).catch(() => {
      if (mountedRef.current) setCurrentVersion("未知");
    });
    return () => {
      mountedRef.current = false;
      const update = updateRef.current;
      updateRef.current = null;
      if (update && !installingRef.current) void update.dispose();
    };
  }, [adapter]);

  const checkForUpdates = async () => {
    if (!adapter.supported) {
      setError("浏览器预览不能检查应用更新，请在 AirDrop 桌面程序中使用此功能。");
      setPhase("error");
      return;
    }
    setError(undefined);
    setPhase("checking");
    try {
      const previous = updateRef.current;
      updateRef.current = null;
      if (previous) await previous.dispose();
      const update = await adapter.check();
      if (!mountedRef.current) {
        if (update) await update.dispose();
        return;
      }
      updateRef.current = update;
      if (!update) {
        setAvailableVersion(undefined);
        setNotes(undefined);
        setPhase("current");
        return;
      }
      setAvailableVersion(update.version);
      setNotes(update.notes?.trim() || undefined);
      setPhase("available");
    } catch (reason) {
      if (!mountedRef.current) return;
      setError(reason instanceof Error ? reason.message : "无法检查更新");
      setPhase("error");
    }
  };

  const installUpdate = async () => {
    const update = updateRef.current;
    if (!update) return;
    setError(undefined);
    setProgress({ downloaded: 0 });
    setPhase("downloading");
    installingRef.current = true;
    try {
      await update.install((nextProgress) => {
        if (!mountedRef.current) return;
        setProgress(nextProgress);
        if (nextProgress.total !== undefined && nextProgress.downloaded >= nextProgress.total) setPhase("installing");
      });
      if (mountedRef.current) setPhase("installing");
    } catch (reason) {
      installingRef.current = false;
      if (!mountedRef.current) return;
      setError(reason instanceof Error ? reason.message : "更新安装失败");
      setPhase("error");
    }
  };

  const busy = phase === "checking" || phase === "downloading" || phase === "installing";
  const progressPercent = progress.total && progress.total > 0
    ? Math.min(100, Math.round(progress.downloaded / progress.total * 100))
    : undefined;

  return <section className="page-section update-section">
    <div className="section-title update-section-title"><div><h2>软件更新</h2><p>Windows 安装版和 Linux AppImage 支持应用内更新</p></div><span className="tag">当前 v{currentVersion}</span></div>
    <div className="card update-card">
      <div className={`update-icon update-${phase}`}><Icon name={phase === "available" ? "download" : phase === "current" ? "check" : "refresh"} size={21} /></div>
      <div className="update-copy">
        <strong>{statusCopy(phase, availableVersion)}</strong>
        {notes && phase === "available" ? <p className="update-notes">{notes}</p> : <p>{error ?? "更新过程不会修改你的设备配对、同步组或外观设置。"}</p>}
        {(phase === "downloading" || phase === "installing") && <div className="update-progress" aria-label="更新下载进度">
          <span style={{ width: `${progressPercent ?? 8}%` }} />
          <small>{progressPercent === undefined ? `${formatBytes(progress.downloaded)} 已下载` : `${progressPercent}% · ${formatBytes(progress.downloaded)} / ${formatBytes(progress.total ?? 0)}`}</small>
        </div>}
      </div>
      <div className="update-actions">
        {phase === "available" ? <button type="button" className="button primary" onClick={() => void installUpdate()}><Icon name="download" size={15} />下载并安装</button> : <button type="button" className="button" disabled={busy} onClick={() => void checkForUpdates()}><Icon name="refresh" size={15} />{phase === "checking" ? "检查中" : phase === "error" ? "重新检查" : "检查更新"}</button>}
      </div>
    </div>
  </section>;
}
