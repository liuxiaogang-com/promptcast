# PromptCast

> 仓库地址：<https://cnb.cool/liuxiaogang/promptcast>

跨平台桌面提示词管理工具：全局快捷键唤起、模糊搜索、分类标签、一键注入到任何输入框。

> macOS 11+ / Windows 10 1809+ · Tauri 2 · Vue 3 · ~8 MB 安装包

## 功能

- **全局快捷键唤起**：在任何应用中按下自定义快捷键即弹出抽屉
- **一键注入**：选中提示词回车 → 自动填入上一个聚焦窗口的输入框（剪贴板 + 模拟 ⌘V/Ctrl+V）
- **失败回退**：注入失败时静默回退仅复制 + Toast 提示
- **分类与标签**：单层文件夹（可拖拽排序）+ 多对多标签
- **模糊搜索**：搜索域 = 标题 + 内容 + 标签
- **置顶 / 收藏**：双独立维度，按使用频率/创建/更新/标题排序
- **预览窗口**：独立窗口里渲染 Markdown
- **网址快捷栏**：抽屉底部一行 favicon，自动抓取，点击在浏览器打开
- **本地 SQLite** + JSON 导入导出（合并 / 替换两种模式）

## 键盘交互（在抽屉里）

| 按键 | 动作 |
|---|---|
| `↑` / `↓` | 切换列表选中项 |
| `Enter` | 注入选中项（默认）/ 仅复制（设置中可改） |
| `⌘C` / `Ctrl+C` | 仅复制 |
| `⌘E` / `Ctrl+E` | 编辑选中 |
| `⌘N` / `Ctrl+N` | 新建 |
| `⌘F` / `Ctrl+F` | 聚焦搜索框 |
| `Tab` / `Shift+Tab` | 在分类 chips 间循环切换 |
| `Space` | 打开预览窗口 |
| `Esc` | 隐藏抽屉（已 pin 时无效） |
| `⌘S` / `Ctrl+S` | 保存（在编辑窗口） |

## 开发

```bash
pnpm install
pnpm tauri dev

# macOS 需要验证辅助功能授权时：
pnpm dev:mac
```

`dev:mac` 保留前端热更新；修改 Rust 后需退出并重新运行此命令。它只生成本地调试应用，不生成安装包或发布版本。

打包：

```bash
# macOS：
pnpm tauri build --bundles dmg
# 输出：src-tauri/target/release/bundle/dmg/*.dmg
# Windows：
pnpm tauri build --bundles nsis
# 输出：src-tauri/target/release/bundle/nsis/*.exe
```

## 首次使用

1. 启动后会显示 400×720 的主抽屉；默认快捷键为 macOS `⌘⇧P`、Windows `Ctrl+Shift+P`
2. 可点击右上 ⚙ 进入「设置 → 快捷键」修改或清空快捷键
3. **macOS**：未授权时启动会显示独立授权引导，点击「打开辅助功能设置」，将引导中的应用图标拖入列表并开启开关；也可点击「在访达中显示」定位文件。macOS 请用 `pnpm dev:mac` 启动独立的 `PromptCast Dev.app` 后授权；裸命令行进程的权限可能归属于启动工具。开发应用使用临时签名，Rust 重新编译后可能需要重新授权。未授权时注入回退为仅复制
4. **Windows**：通常无需额外授权；如果想注入到以管理员权限运行的窗口（如管理员命令行），需要把 PromptCast 也以管理员权限启动（UIPI 限制）

## 项目结构

```
src/         前端 Vue 3（Options API）
src-tauri/   后端 Rust + Tauri 2
docs/        架构文档与里程碑
prototype/   pencil 原型源文件（用 pencil.app 打开）
```

设计架构详见 `docs/PLAN.md`，里程碑与待办详见 `docs/TODO.md`。

## 已知限制（v0.1.0）

- 抽屉弹出时会短暂获得焦点（NSPanel/`WS_EX_NOACTIVATE` 留待后续）；目前流程"按 Enter → 立即隐藏 → 等 80ms → 模拟粘贴"已可用
- 变量占位符 `{{var}}` 暂未实现，列入 v2
- 无云同步；多设备请通过「设置 → 数据」导出 JSON 后手动同步

## License

MIT
