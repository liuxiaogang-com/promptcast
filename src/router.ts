// router.ts — 多窗口共用一个 Vue bundle，路由按 hash 区分。
// 每个 Tauri WebviewWindow 加载 index.html#/<route>。
import { createRouter, createWebHashHistory, type RouteRecordRaw } from "vue-router";

const routes: RouteRecordRaw[] = [
  {
    path: "/changelog",
    name: "changelog",
    component: () => import("./views/ChangelogView.vue"),
  },
  {
    path: "/permissions",
    name: "permissions",
    component: () => import("./views/PermissionsView.vue"),
  },
  { path: "/", redirect: "/drawer" },
  {
    path: "/drawer",
    name: "drawer",
    component: () => import("./views/DrawerView.vue"),
  },
  {
    path: "/preview/:id?",
    name: "preview",
    component: () => import("./views/PreviewView.vue"),
  },
  {
    path: "/editor/:id?",
    name: "editor",
    component: () => import("./views/EditorView.vue"),
  },
  {
    path: "/settings",
    name: "settings",
    component: () => import("./views/SettingsView.vue"),
  },
];

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
});
