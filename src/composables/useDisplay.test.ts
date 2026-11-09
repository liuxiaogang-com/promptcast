import { it, expect, vi } from "vitest";
import { applyFontScale, FONT_SIZE_OPTIONS, initDisplay } from "./useDisplay";
import { settingsGetAll } from "../api/settings";
import { listenAppEvent } from "./useAppEvents";
import { DEFAULT_SETTINGS } from "../types/settings";

vi.mock("../api", () => ({ ensureBackendReady: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../api/settings", () => ({ settingsGetAll: vi.fn() }));
vi.mock("./useAppEvents", () => ({ EVT_SETTINGS_CHANGED: "settings-changed", listenAppEvent: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("../utils/logger", () => ({ log: { error: vi.fn() } }));

it("档位使用统一比例，无效值回到默认，并响应跨窗口设置变化", async () => {
  for (const option of FONT_SIZE_OPTIONS) {
    applyFontScale(option.value);
    expect(document.documentElement.style.getPropertyValue("--font-scale")).toBe(String(option.value));
  }
  applyFontScale(NaN);
  expect(document.documentElement.style.getPropertyValue("--font-scale")).toBe("1");
  vi.mocked(settingsGetAll).mockResolvedValue({ ...DEFAULT_SETTINGS, font_scale: 1.2 });
  await initDisplay();
  expect(document.documentElement.style.getPropertyValue("--font-scale")).toBe("1.2");
  vi.mocked(settingsGetAll).mockResolvedValue(DEFAULT_SETTINGS);
  const handler = vi.mocked(listenAppEvent).mock.calls[0][1];
  handler("font_scale");
  await vi.waitFor(() => expect(document.documentElement.style.getPropertyValue("--font-scale")).toBe("1"));
  window.dispatchEvent(new Event("beforeunload"));
});
