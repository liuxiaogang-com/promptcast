// types/clipboard.ts — 剪贴板历史条目（仅文本），与 Rust ClipEntry 对齐。
export interface ClipEntry {
  id: number;
  image_id?: string | null;
  saved_prompt_id?: number | null;
  content: string;
  char_count: number;
  created_at: number; // Unix ms
}
