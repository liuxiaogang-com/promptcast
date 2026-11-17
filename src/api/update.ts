// api/update.ts
import { invoke } from "@tauri-apps/api/core";
import type { UpdateInfo, ReleaseNotes } from "../types/update";

// 查更新：null = 未配置 / 已是最新；本平台无包返回错误。
export const updateCheck = () => invoke<UpdateInfo | null>("update_check");
// 下载本平台安装包（发 update-progress 事件），不安装或退出应用。
export const updateDownload = (expectedVersion: string) =>
  invoke<void>("update_download", { expectedVersion });
export const updateChangelog = () =>
  invoke<ReleaseNotes[]>("update_changelog");
export const updateGetManifestUrl = () =>
  invoke<string>("update_get_manifest_url");
export const updateSetManifestUrl = (url: string) =>
  invoke<void>("update_set_manifest_url", { url });

export const updateInstall = (expectedVersion: string) => invoke<void>("update_install", { expectedVersion });
export const updateStatus = () => invoke<{ info: UpdateInfo | null; ready: boolean; busy: boolean }>("update_status");
export const windowOpenUpdate = () => invoke<void>("window_open_update");
