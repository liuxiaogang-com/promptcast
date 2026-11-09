// GitHub 构建完成后生成供 CNB 同步的元数据，不修改线上更新入口。
import { readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { join } from "node:path";

const tag = process.env.GITHUB_REF_NAME;
if (!/^v\d+\.\d+\.\d+$/.test(tag ?? "")) throw new Error("必须从正式版本 tag 发版");
const version = tag.slice(1);
const packageVersion = JSON.parse(readFileSync("package.json", "utf8")).version;
const tauriVersion = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8")).version;
const rustVersion = readFileSync("src-tauri/Cargo.toml", "utf8").match(/^version = "([^"]+)"/m)?.[1];
if ([packageVersion, tauriVersion, rustVersion].some((v) => v !== version)) throw new Error("tag 与三处应用版本不一致");
const source = readFileSync(`releases/${tag}.md`, "utf8").trim();
const [heading, ...body] = source.split("\n");
const title = heading.replace(/^#\s+/, "");
const notes = body.join("\n").trim();
const assets = readdirSync("assets")
  .filter((name) => /\.(dmg|exe|msi)$/.test(name))
  .map((name) => ({
    name,
    size: statSync(join("assets", name)).size,
    sha256: createHash("sha256").update(readFileSync(join("assets", name))).digest("hex"),
  }));
const dmg = `PromptCast_${version}_universal.dmg`;
const exe = `PromptCast_${version}_x64-setup.exe`;
const downloads = {};
for (const [platform, name] of Object.entries({
  "darwin-aarch64": dmg, "darwin-x86_64": dmg, "windows-x86_64": exe,
})) {
  const asset = assets.find((a) => a.name === name);
  if (!asset) throw new Error(`缺少安装包: ${name}`);
  downloads[platform] = { name, sha256: asset.sha256 };
}
writeFileSync("release.json", JSON.stringify({
  version, title, notes, pub_date: new Date().toISOString(), assets, downloads,
}, null, 2) + "\n");
