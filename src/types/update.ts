// types/update.ts — 与 Rust update::UpdateInfo 对齐。
// 注意：不含下载 url / sha —— 真正下载由后端按平台重新读清单，前端只拿展示信息。
export interface UpdateInfo {
  version: string;
  current: string;
  title: string;
  notes: string;
  pub_date: string | null;
}

// update-progress 事件载荷。total 为 null 表示服务端没给 content-length。
export interface UpdateProgress {
  downloaded: number;
  total: number | null;
}

export interface ReleaseNotes {
  version: string;
  title: string;
  notes: string;
  pub_date: string | null;
}
