#!/usr/bin/env node
// check-ipc-contract.mjs — 校验前端 invoke 调用与 Rust #[tauri::command] 的契约：
//   1) 每个 invoke("cmd") 都已在 lib.rs generate_handler! 注册；
//   2) JS 传的参数键集合 == Rust 命令参数名（剔除注入型 State/AppHandle/Window 后转 camelCase）。
// Tauri 默认把 JS 的 camelCase 键映射到 Rust 的 snake_case 参数；写错就运行时静默失败，
// 而 vue-tsc 抓不到（invoke 第二参是 any）。此脚本把这类 bug 提前到构建期。
import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const INJECTED = /^(State|AppHandle|Window|WebviewWindow|AppData|tauri::)/;

const snakeToCamel = (s) => s.replace(/_([a-z0-9])/g, (_, c) => c.toUpperCase());

function walk(dir, out = []) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) {
      if (!/node_modules|target|dist|gen/.test(e.name)) walk(p, out);
    } else out.push(p);
  }
  return out;
}

// 在 from 之后找到首个 `{`，返回其平衡闭合内部文本（不含外层花括号）。
function balancedObject(text, from) {
  const start = text.indexOf("{", from);
  if (start < 0) return null;
  let depth = 0;
  for (let i = start; i < text.length; i++) {
    if (text[i] === "{") depth++;
    else if (text[i] === "}") {
      depth--;
      if (depth === 0) return text.slice(start + 1, i);
    }
  }
  return null;
}

// 顶层逗号分割（尊重 {} [] <> () 嵌套）。
function splitTop(s) {
  const parts = [];
  let depth = 0, cur = "";
  for (const ch of s) {
    if ("{[<(".includes(ch)) depth++;
    else if ("}]>)".includes(ch)) depth--;
    if (ch === "," && depth === 0) { parts.push(cur); cur = ""; }
    else cur += ch;
  }
  if (cur.trim()) parts.push(cur);
  return parts;
}

// ---- 1) 收集前端 invoke ----
const jsCalls = []; // { cmd, keys:Set, file }
for (const f of walk(join(ROOT, "src"))) {
  if (!/\.(ts|vue)$/.test(f)) continue;
  const src = readFileSync(f, "utf8");
  const re = /invoke\s*(?:<[^>]*>)?\s*\(\s*"([a-z0-9_]+)"/g;
  let m;
  while ((m = re.exec(src))) {
    const cmd = m[1];
    // 命令名之后是否还有第二个参数对象
    const after = src.slice(m.index + m[0].length);
    let keys = new Set();
    const comma = after.search(/^\s*,/);
    if (comma === 0) {
      const obj = balancedObject(after, 0);
      if (obj != null) {
        for (const part of splitTop(obj)) {
          const key = part.split(":")[0].trim();
          if (key) keys.add(key);
        }
      }
    }
    jsCalls.push({ cmd, keys, file: f });
  }
}

// ---- 2) 收集 Rust #[tauri::command] ----
const rustCmds = new Map(); // cmd -> Set(camelCase param names)
for (const f of walk(join(ROOT, "src-tauri", "src"))) {
  if (!f.endsWith(".rs")) continue;
  const src = readFileSync(f, "utf8");
  const re = /#\[tauri::command(?:\([^\]]*\))?\][^\n]*\n\s*(?:pub\s+)?fn\s+([a-z0-9_]+)\s*\(([^)]*)\)/g;
  let m;
  while ((m = re.exec(src))) {
    const name = m[1];
    const params = splitTop(m[2]);
    const keys = new Set();
    for (const p of params) {
      const t = p.trim();
      if (!t) continue;
      const colon = t.indexOf(":");
      if (colon < 0) continue;
      const pname = t.slice(0, colon).trim().replace(/^mut\s+/, "");
      const ptype = t.slice(colon + 1).trim();
      if (INJECTED.test(ptype)) continue;
      keys.add(snakeToCamel(pname));
    }
    rustCmds.set(name, keys);
  }
}

// ---- 3) 注册表（lib.rs generate_handler!）----
const lib = readFileSync(join(ROOT, "src-tauri", "src", "lib.rs"), "utf8");
const handlerBlock = lib.slice(lib.indexOf("generate_handler!"));
const registered = new Set(
  [...handlerBlock.matchAll(/(?:commands::\w+::|logging::|)?([a-z0-9_]+)\s*[,\]]/g)].map((x) => x[1]),
);

// ---- 4) 比对 ----
const errors = [];
const ignore = new Set(["ping", "log_record", "log_dir"]); // 自检/日志命令
for (const { cmd, keys, file } of jsCalls) {
  if (ignore.has(cmd)) continue;
  if (!rustCmds.has(cmd)) {
    errors.push(`✗ invoke("${cmd}") 无对应 #[tauri::command]（${file}）`);
    continue;
  }
  if (!registered.has(cmd)) {
    errors.push(`✗ "${cmd}" 未在 lib.rs generate_handler! 注册`);
  }
  const want = rustCmds.get(cmd);
  const got = keys;
  const missing = [...want].filter((k) => !got.has(k));
  const extra = [...got].filter((k) => !want.has(k));
  if (missing.length || extra.length) {
    errors.push(
      `✗ "${cmd}" 参数键不匹配：JS=[${[...got].join(",")}] vs Rust=[${[...want].join(",")}]` +
        (missing.length ? ` 缺:[${missing}]` : "") + (extra.length ? ` 多:[${extra}]` : ""),
    );
  }
}

if (errors.length) {
  console.error("IPC 契约校验失败：\n" + errors.join("\n"));
  process.exit(1);
}
console.log(`IPC 契约 OK：${jsCalls.length} 处 invoke 全部匹配 ${rustCmds.size} 个命令。`);
