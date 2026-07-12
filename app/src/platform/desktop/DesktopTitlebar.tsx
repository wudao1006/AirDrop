import { getCurrentWindow } from "@tauri-apps/api/window";

export function DesktopTitlebar() {
  const run = (action: (appWindow: ReturnType<typeof getCurrentWindow>) => Promise<void>) => void action(getCurrentWindow()).catch(() => undefined);
  const keepControlInteractive = (event: React.PointerEvent<HTMLButtonElement>) => event.stopPropagation();

  return <header className="window-titlebar">
    <div className="window-titlebar-drag" data-tauri-drag-region />
    <div className="window-titlebar-brand" data-tauri-drag-region><i /></div>
    <div className="window-titlebar-title" data-tauri-drag-region>AirDrop</div>
    <div className="window-controls">
      <button type="button" aria-label="最小化" title="最小化" onPointerDown={keepControlInteractive} onClick={() => run((appWindow) => appWindow.minimize())}><span className="control-minimize" /></button>
      <button type="button" aria-label="最大化" title="最大化" onPointerDown={keepControlInteractive} onClick={() => run((appWindow) => appWindow.toggleMaximize())}><span className="control-maximize" /></button>
      <button type="button" className="control-close" aria-label="关闭" title="关闭" onPointerDown={keepControlInteractive} onClick={() => run((appWindow) => appWindow.close())}><span /></button>
    </div>
  </header>;
}
