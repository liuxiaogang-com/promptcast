import { beforeEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useUpdateStore } from "./update";
import { updateCheck, updateDownload, updateInstall, updateStatus, windowOpenUpdate } from "../api/update";
vi.mock("../api/update", () => ({
  updateCheck: vi.fn(), updateDownload: vi.fn(), updateInstall: vi.fn(),
  updateStatus: vi.fn(), windowOpenUpdate: vi.fn(), updateGetManifestUrl: vi.fn(), updateSetManifestUrl: vi.fn(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
const info = { version: "0.2.0", current: "0.1.1", title: "更新", notes: "", pub_date: null };
beforeEach(() => { localStorage.clear(); vi.clearAllMocks(); setActivePinia(createPinia()); });
it("自动检查调用独立窗口，不依赖抽屉显示；跨窗口忽略偏好仍有效", async () => {
  vi.mocked(updateCheck).mockResolvedValue(info);
  const store = useUpdateStore();
  expect(await store.check()).toBe(true);
  expect(windowOpenUpdate).toHaveBeenCalledTimes(1);
  localStorage.setItem("pc.update.skipVersion", info.version);
  expect(await store.check()).toBe(false);
  expect(windowOpenUpdate).toHaveBeenCalledTimes(1);
  expect(await store.check(true)).toBe(true);
  expect(windowOpenUpdate).toHaveBeenCalledTimes(2);
});
it("下载完成不启动安装，只有用户确认才安装，失败保留错误", async () => {
  const store = useUpdateStore();
  store.info = info;
  await store.startInstall();
  expect(updateInstall).not.toHaveBeenCalled();
  await store.startDownload();
  expect(updateDownload).toHaveBeenCalledWith(info.version);
  expect(store.ready).toBe(true);
  expect(store.launched).toBe(false);
  expect(updateInstall).not.toHaveBeenCalled();
  vi.mocked(updateStatus).mockResolvedValue({ info, ready: true, busy: false });
  vi.mocked(updateInstall).mockRejectedValueOnce(new Error("无法写入应用目录"));
  await store.startInstall();
  expect(store.error).toContain("无法写入应用目录");
  expect(store.launched).toBe(false);
  expect(store.installing).toBe(false);
  await store.startInstall();
  expect(store.launched).toBe(true);
});
