// types/settings.ts — settings 表是 key/value，前端用强类型 Settings 包装。
import type { SortMode } from "./prompt";

export type DefaultAction = "inject" | "copy_only";
export type ThemeMode = "system" | "light" | "dark";

export interface Settings {
  hotkey: string | null; // 例如 "CmdOrCtrl+Shift+P"
  theme: ThemeMode;
  default_action: DefaultAction;
  pin_default: boolean;
  sort_mode: SortMode;
  auto_start: boolean;
  accessibility_granted: boolean; // mac 辅助功能是否授权
  clipboard_history_enabled: boolean; // 是否记录剪贴板历史
  clipboard_history_limit: number; // 保留条数上限
}

export const DEFAULT_SETTINGS: Settings = {
  hotkey: "CmdOrCtrl+Shift+P",
  theme: "system",
  default_action: "inject",
  pin_default: false,
  sort_mode: "recent_used",
  auto_start: false,
  accessibility_granted: false,
  clipboard_history_enabled: true,
  clipboard_history_limit: 500,
};
