import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { permissionsAppTarget, permissionsCheckAccessibility } from "../api/inject";
import PermissionsView from "./PermissionsView.vue";

vi.mock("../api", () => ({ ensureBackendReady: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../api/inject", () => ({
  permissionsAppTarget: vi.fn(),
  permissionsCheckAccessibility: vi.fn(),
  permissionsRequestAccessibility: vi.fn(),
  permissionsDragApp: vi.fn(),
}));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ isVisible: async () => true, close: vi.fn() }),
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ revealItemInDir: vi.fn() }));
vi.mock("../utils/format", () => ({ isMac: () => true }));
vi.mock("../stores/settings", () => ({
  useSettingsStore: () => ({ loadAll: async () => {}, data: { theme: "system" } }),
}));
vi.mock("../composables/useTheme", () => ({ applyPersistedTheme: vi.fn() }));

describe("PermissionsView", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(permissionsAppTarget).mockResolvedValue({ path: "/Applications/PromptCast.app", bundled: true });
    vi.mocked(permissionsCheckAccessibility).mockResolvedValue(false);
  });
  afterEach(() => { vi.useRealTimers(); vi.clearAllMocks(); });

  it("路径读取失败不影响自动检测，授权和撤销都会切换状态样式", async () => {
    vi.mocked(permissionsAppTarget).mockRejectedValueOnce(new Error("path unavailable"));
    const wrapper = mount(PermissionsView);
    try {
      await flushPromises();
      expect(wrapper.find(".permission-status.denied").exists()).toBe(true);
      vi.mocked(permissionsCheckAccessibility).mockResolvedValue(true);
      await vi.advanceTimersByTimeAsync(1500);
      await flushPromises();
      expect(wrapper.get(".permission-status.granted").text()).toContain("自动粘贴可用");
      expect(wrapper.find(".drag-card").exists()).toBe(false);
      vi.mocked(permissionsCheckAccessibility).mockResolvedValue(false);
      await vi.advanceTimersByTimeAsync(1500);
      await flushPromises();
      expect(wrapper.find(".permission-status.denied").exists()).toBe(true);
    } finally { wrapper.unmount(); }
  });

  it("手动检测显示进度和明确结果，失败后重试会清除旧错误", async () => {
    const wrapper = mount(PermissionsView);
    try {
      await flushPromises();
      let resolveCheck!: (value: boolean) => void;
      vi.mocked(permissionsCheckAccessibility).mockImplementationOnce(() => new Promise((resolve) => { resolveCheck = resolve; }));
      await wrapper.get(".check-feedback button").trigger("click");
      expect(wrapper.get(".permission-status.checking").text()).toContain("正在检测");
      resolveCheck(false);
      await flushPromises();
      expect(wrapper.get(".permission-status.denied").text()).toContain("系统仍未允许当前进程");
      expect(wrapper.text()).not.toContain("最近检测");
      vi.mocked(permissionsCheckAccessibility).mockRejectedValueOnce(new Error("IPC unavailable"));
      await wrapper.get(".check-feedback button").trigger("click");
      await flushPromises();
      expect(wrapper.get(".permission-status.error").text()).toContain("IPC unavailable");
      vi.mocked(permissionsCheckAccessibility).mockResolvedValue(true);
      await wrapper.get(".check-feedback button").trigger("click");
      await flushPromises();
      expect(wrapper.find(".permission-status.granted").exists()).toBe(true);
      expect(wrapper.text()).not.toContain("IPC unavailable");
    } finally { wrapper.unmount(); }
  });
});
