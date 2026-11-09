import { ensureBackendReady } from "../api";
import { settingsGetAll } from "../api/settings";
import { listenAppEvent, EVT_SETTINGS_CHANGED } from "./useAppEvents";
import { log } from "../utils/logger";

export const FONT_SIZE_OPTIONS = [
  { label: "默认", value: 1 },
  { label: "较大", value: 1.2 },
] as const;

export function applyFontScale(value: number): void {
  const scale = Number.isFinite(value) && value >= 1 && value <= 1.6 ? value : 1;
  document.documentElement.style.setProperty("--font-scale", String(scale));
}

export async function initDisplay(): Promise<void> {
  await ensureBackendReady();
  let revision = 0;
  const refresh = async () => {
    const current = ++revision;
    const settings = await settingsGetAll();
    if (current === revision) applyFontScale(settings.font_scale);
  };
  const unlisten = await listenAppEvent<string>(EVT_SETTINGS_CHANGED, (key) => {
    if (key === "font_scale" || key === "*") {
      void refresh().catch((e) => log.error(`display settings refresh failed: ${e}`));
    }
  });
  window.addEventListener("beforeunload", unlisten, { once: true });
  await refresh();
}
