// main.ts — Vue 入口。
import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import { router } from "./router";
import { initDisplay } from "./composables/useDisplay";
import { initTheme } from "./composables/useTheme";
import { configureLogger, installGlobalErrorHandlers, log } from "./utils/logger";

import "./styles/reset.css";
import "./styles/tokens.css";
import "./styles/theme-light.css";
import "./styles/theme-dark.css";
import "./styles/global.css";

// 根据 hash 路由判定窗口身份，便于日志区分来源
const route = window.location.hash.replace(/^#\//, "").split("/")[0] || "drawer";
configureLogger(route);
installGlobalErrorHandlers();
log.info(`window booted: ${route}`);

initTheme();
void initDisplay().catch((e) => log.error(`display settings failed: ${e}`));

createApp(App).use(createPinia()).use(router).mount("#app");
