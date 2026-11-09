// types/prompt.ts — 与 src-tauri/src/models/prompt.rs 一一对应。
export interface Prompt {
  id: number;
  title: string;
  content: string;
  rich_content?: Record<string, unknown> | null;
  folder_id: number | null;
  tag_ids: number[];
  is_favorite: boolean;
  is_pinned: boolean;
  use_count: number;
  last_used_at: number | null; // unix ms
  created_at: number;
  updated_at: number;
}

export interface PromptDraft {
  title: string;
  content: string;
  rich_content?: Record<string, unknown> | null;
  folder_id: number | null;
  tag_ids: number[];
}

export type SortMode =
  | "recent_used"
  | "created"
  | "updated"
  | "title";
