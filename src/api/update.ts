// api/update.ts
import { invoke } from "@tauri-apps/api/core";
import type { UpdateInfo, ReleaseNotes } from "../types/update";

// 查更新：null = 未配置 / 已是最新；本平台无包返回错误。
export const updateCheck = () => invoke<UpdateInfo | null>("update_check");
// 下载本平台安装包（发 update-progress 事件）并拉起安装器。
export const updateDownloadInstall = (expectedVersion: string) =>
  invoke<void>("update_download_install", { expectedVersion });
export const updateChangelog = () =>
  invoke<ReleaseNotes[]>("update_changelog");
export const updateGetManifestUrl = () =>
  invoke<string>("update_get_manifest_url");
export const updateSetManifestUrl = (url: string) =>
  invoke<void>("update_set_manifest_url", { url });
