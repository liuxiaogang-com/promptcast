import { invoke } from "@tauri-apps/api/core";
import type { Prompt } from "../types/prompt";
import type { InjectResult } from "./inject";

export interface ImageAsset { id: string; data_url: string; width: number; height: number }
export const imageGet = (id: string, thumbnail = false) => invoke<ImageAsset>("image_get", { id, thumbnail });
export const imageImport = (data: string) => invoke<string>("image_import", { data });
export const clipboardSave = (id: number, favorite = false) => invoke<Prompt>("clipboard_save", { id, favorite });
export const contentCopy = (id: number, source: "prompt" | "clipboard", paste = false) =>
  invoke<InjectResult>("content_copy", { id, source, paste });

export async function importImageFile(file: File): Promise<string> {
  if (file.size > 20 * 1024 * 1024) throw new Error("图片不能超过 20 MB");
  const data = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result).split(",")[1]);
    reader.onerror = () => reject(new Error("图片读取失败"));
    reader.readAsDataURL(file);
  });
  return imageImport(data);
}
