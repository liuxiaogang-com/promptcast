// api/window.ts
import { invoke } from "@tauri-apps/api/core";

export const windowShowDrawer = () => invoke<void>("window_show_drawer");
export const windowHideDrawer = () => invoke<void>("window_hide_drawer");
export const windowSetPin = (pinned: boolean) =>
  invoke<void>("window_set_pin", { pinned });
export const windowSetModalOpen = (open: boolean) =>
  invoke<void>("window_set_modal_open", { open });
export const windowOpenPreview = (id: number) =>
  invoke<{ label: string }>("window_open_preview", { id });
export const windowOpenEditor = (id: number | null) =>
  invoke<{ label: string }>("window_open_editor", { id });
export const windowOpenSettings = () =>
  invoke<{ label: string }>("window_open_settings");

export const registerHotkey = (shortcut: string) =>
  invoke<void>("register_hotkey_cmd", { shortcut });
export const unregisterHotkey = () =>
  invoke<void>("unregister_hotkey_cmd");

export const windowOpenPermissions = () =>
  invoke<{ label: string }>("window_open_permissions");
