import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";

export interface UpdateProgress {
  downloaded: number;
  total?: number;
}

export interface AvailableUpdate {
  version: string;
  notes?: string;
  install(onProgress: (progress: UpdateProgress) => void): Promise<void>;
  dispose(): Promise<void>;
}

export interface UpdaterAdapter {
  readonly supported: boolean;
  getCurrentVersion(): Promise<string>;
  check(): Promise<AvailableUpdate | null>;
}

const toAvailableUpdate = (update: Update): AvailableUpdate => ({
  version: update.version,
  notes: update.body,
  async install(onProgress) {
    let downloaded = 0;
    let total: number | undefined;
    const report = (event: DownloadEvent) => {
      if (event.event === "Started") {
        total = event.data.contentLength;
        onProgress({ downloaded, total });
      } else if (event.event === "Progress") {
        downloaded += event.data.chunkLength;
        onProgress({ downloaded, total });
      } else {
        onProgress({ downloaded: total ?? downloaded, total });
      }
    };
    await update.downloadAndInstall(report, { timeout: 10 * 60_000 });
    await relaunch();
  },
  dispose: () => update.close(),
});

export const createUpdaterAdapter = (): UpdaterAdapter => {
  const supported = typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);
  return {
    supported,
    getCurrentVersion: () => supported ? getVersion() : Promise.resolve("浏览器预览"),
    async check() {
      if (!supported) return null;
      const update = await check({ timeout: 20_000 });
      return update ? toAvailableUpdate(update) : null;
    },
  };
};
