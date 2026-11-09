// CNB 主动拉取 GitHub 正式 Release；只在附件公开可下载并通过校验后发布清单。
import { createHash } from "node:crypto";
import { createReadStream, createWriteStream, existsSync, readFileSync, writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Readable, Transform } from "node:stream";
import { pipeline } from "node:stream/promises";
import { execFileSync } from "node:child_process";
import { pathToFileURL } from "node:url";

const GITHUB_REPO = "liuxiaogang-com/promptcast";
const CNB_REPO = "liuxiaogang/promptcast";
const CNB_API = `https://api.cnb.cool/${CNB_REPO}/-/releases`;
const CNB_PUBLIC = `https://cnb.cool/${CNB_REPO}/-/releases`;

function versionParts(version) {
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error(`不是正式版本: ${version}`);
  return version.split(".").map(Number);
}
export function compareVersions(a, b) {
  const av = versionParts(a), bv = versionParts(b);
  for (let i = 0; i < 3; i++) if (av[i] !== bv[i]) return av[i] - bv[i];
  return 0;
}
export function validateManifest(manifest, release) {
  if (`v${manifest.version}` !== release.tag_name) throw new Error("Release 与清单版本不一致");
  versionParts(manifest.version);
  if (!manifest.title || typeof manifest.notes !== "string" || !manifest.pub_date) throw new Error("发布日志不完整");
  if (!Array.isArray(manifest.assets) || !manifest.assets.length) throw new Error("缺少附件");
  const names = new Set();
  for (const asset of manifest.assets) {
    if (!/^[a-zA-Z0-9_.-]+\.(dmg|exe|msi|tar\.gz)$/.test(asset.name) || names.has(asset.name)) throw new Error("附件名无效或重复");
    names.add(asset.name);
    if (!/^[a-f0-9]{64}$/.test(asset.sha256) || !Number.isSafeInteger(asset.size) || asset.size <= 0) throw new Error("附件校验信息无效");
    if (!release.assets.some((a) => a.name === asset.name && a.size === asset.size)) throw new Error(`GitHub 附件未齐全: ${asset.name}`);
  }
  for (const platform of ["darwin-aarch64", "darwin-x86_64", "windows-x86_64"]) {
    const download = manifest.downloads?.[platform];
    if (!manifest.assets.some((a) => a.name === download?.name && a.sha256 === download?.sha256)) throw new Error(`缺少平台安装包: ${platform}`);
  }
}
export function publication(manifest, previousLog, previousUpdate) {
  const { version, title, notes, pub_date } = manifest;
  const entry = { version, title, notes, pub_date };
  const releases = [...previousLog.releases.filter((r) => r.version !== version), entry]
    .sort((a, b) => compareVersions(b.version, a.version));
  // 重试旧版本不能让更新入口倒退；保留 title/notes 兼容旧客户端。
  const update = previousUpdate && compareVersions(previousUpdate.version, version) > 0
    ? previousUpdate
    : { ...entry, downloads: Object.fromEntries(Object.entries(manifest.downloads).map(([platform, asset]) => [platform, {
        url: `${CNB_PUBLIC}/download/v${version}/${encodeURIComponent(asset.name)}`,
        sha256: asset.sha256,
      }])) };
  return { changelog: { releases }, update };
}

async function request(url, options = {}) {
  if (new URL(url).protocol !== "https:") throw new Error("只允许 HTTPS");
  const response = await fetch(url, { signal: AbortSignal.timeout(300_000), ...options });
  if (!response.ok) throw new Error(`HTTP ${response.status}: ${new URL(url).pathname}`);
  return response;
}
async function cnb(path, method = "GET", body) {
  const url = path.startsWith("https:") ? path : `${CNB_API}${path}`;
  // 上传确认 URL 来自服务端，不能把临时令牌发送到其他站点。
  if (!url.startsWith(`${CNB_API}/`) && url !== CNB_API) throw new Error("无效的 CNB API 地址");
  const response = await request(url, {
    method, redirect: "error",
    headers: { Authorization: `Bearer ${process.env.CNB_TOKEN}`, "Content-Type": "application/json", Accept: "application/vnd.cnb.api+json" },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  const text = await response.text();
  return text ? JSON.parse(text) : null;
}
async function verifiedDownload(url, asset, path, authenticated = false) {
  let response;
  if (authenticated) {
    // 草稿附件的重试校验：鉴权只发给 CNB API，跳转后的对象存储不带令牌。
    const apiUrl = url.replace(CNB_PUBLIC, CNB_API);
    response = await fetch(apiUrl, { headers: { Authorization: `Bearer ${process.env.CNB_TOKEN}` },
      redirect: "manual", signal: AbortSignal.timeout(300_000) });
    if ([301, 302, 303, 307, 308].includes(response.status)) {
      const location = response.headers.get("location");
      if (!location) throw new Error("下载跳转缺少地址");
      response = await request(new URL(location, apiUrl).href);
    }
    if (!response.ok) throw new Error(`草稿附件下载失败: HTTP ${response.status}`);
  } else {
    response = await request(url);
  }
  const hash = createHash("sha256");
  let size = 0;
  const meter = new Transform({ transform(chunk, _encoding, callback) {
    hash.update(chunk); size += chunk.length; callback(null, chunk);
  } });
  if (path) {
    await pipeline(Readable.fromWeb(response.body), meter, createWriteStream(path));
  } else {
    for await (const chunk of Readable.fromWeb(response.body)) { hash.update(chunk); size += chunk.length; }
  }
  if (size !== asset.size || hash.digest("hex") !== asset.sha256) throw new Error(`附件校验失败: ${asset.name}`);
}
function readJson(path, fallback) { return existsSync(path) ? JSON.parse(readFileSync(path, "utf8")) : fallback; }
function git(...args) {
  // 凭据只在子进程环境中传递，不写入远端 URL、命令参数或仓库配置。
  const auth = Buffer.from(`cnb:${process.env.CNB_TOKEN}`).toString("base64");
  return execFileSync("git", args, { encoding: "utf8", env: {
    ...process.env, GIT_CONFIG_COUNT: "1", GIT_CONFIG_KEY_0: "http.https://cnb.cool/.extraheader",
    GIT_CONFIG_VALUE_0: `Authorization: Basic ${auth}`,
  } });
}

async function main() {
  if (!process.env.CNB_TOKEN) throw new Error("必须在 CNB 流水线内运行");
  // 取锁后拉最新 main，避免排队任务使用旧 checkout 覆盖已发布日志。
  git("fetch", "https://cnb.cool/liuxiaogang/promptcast.git", "main");
  git("checkout", "-B", "release-sync", "FETCH_HEAD");
  // 静态索引仅在 GitHub Release 完整发布后更新，不依赖共享出口的匿名 API 配额。
  const index = await (await request(`https://raw.githubusercontent.com/${GITHUB_REPO}/main/release-index.json`)).json();
  const candidates = index.releases.sort((a, b) => compareVersions(a.version, b.version));
  for (const manifest of candidates) {
    const tag = `v${manifest.version}`;
    const release = { tag_name: tag, assets: manifest.assets.map((asset) => ({
      ...asset, browser_download_url: `https://github.com/${GITHUB_REPO}/releases/download/${tag}/${encodeURIComponent(asset.name)}`,
    })) };
    const current = readJson("update.json", null);
    const log = readJson("changelog.json", { releases: [] });
    if (current && compareVersions(current.version, manifest.version) >= 0
        && log.releases.some((r) => r.version === manifest.version)) continue;
    validateManifest(manifest, release);
    console.log(`同步 ${release.tag_name}`);
    // 只同步该 tag 的代码对象，以便 CNB Release 对应正确源码；不覆盖 CNB main。
    git("fetch", `https://github.com/${GITHUB_REPO}.git`, `refs/tags/${release.tag_name}`);
    git("push", "https://cnb.cool/liuxiaogang/promptcast.git", `FETCH_HEAD:refs/tags/${release.tag_name}`);
    let target;
    try { target = await cnb(`/tags/${release.tag_name}`); }
    catch (error) {
      if (!error.message.startsWith("HTTP 404:")) throw error;
      target = await cnb("", "POST", { tag_name: release.tag_name, target_commitish: release.tag_name,
        name: manifest.title, body: manifest.notes, draft: true, prerelease: false, make_latest: "false" });
    }
    const temp = mkdtempSync(join(tmpdir(), "promptcast-sync-"));
    try {
      for (const asset of manifest.assets) {
        const existing = target.assets?.find((a) => a.name === asset.name);
        if (existing) {
          // 已存在附件仍需校验，不能静默覆盖同一版本的安装包。
          if (existing.hash_algo?.toLowerCase() !== "sha256" || existing.hash_value?.toLowerCase() !== asset.sha256) {
            await verifiedDownload(`${CNB_PUBLIC}/download/${release.tag_name}/${encodeURIComponent(asset.name)}`, asset, undefined, target.draft);
          }
          continue;
        }
        const file = join(temp, asset.name);
        const source = release.assets.find((a) => a.name === asset.name);
        await verifiedDownload(source.browser_download_url, asset, file);
        const upload = await cnb(`/${target.id}/asset-upload-url`, "POST", {
          asset_name: asset.name, size: asset.size, overwrite: false, ttl: 0,
        });
        await request(upload.upload_url, { method: "PUT", headers: { "Content-Length": String(asset.size) },
          body: createReadStream(file), duplex: "half", redirect: "error" });
        const verify = new URL(upload.verify_url, "https://api.cnb.cool").href;
        await cnb(verify, "POST");
      }
      await cnb(`/${target.id}`, "PATCH", { draft: false, name: manifest.title, body: manifest.notes,
        make_latest: !current || compareVersions(manifest.version, current.version) >= 0 ? "true" : "false" });
      // 以匿名客户端下载路径验证，确保不是只有流水线令牌能下载。
      for (const asset of manifest.assets) {
        await verifiedDownload(`${CNB_PUBLIC}/download/${release.tag_name}/${encodeURIComponent(asset.name)}`, asset);
      }
      const result = publication(manifest, log, current);
      writeFileSync("changelog.json", JSON.stringify(result.changelog, null, 2) + "\n");
      writeFileSync("update.json", JSON.stringify(result.update, null, 2) + "\n");
      git("add", "changelog.json", "update.json");
      if (git("diff", "--cached", "--name-only").trim()) {
        git("-c", "user.name=release-sync", "-c", "user.email=release-sync@users.noreply.cnb.cool",
          "commit", "-m", `chore(release): 同步 ${release.tag_name} [skip ci]`);
        // 非快进时直接失败，下次定时任务重新读最新 main；不强推或覆盖并发提交。
        git("push", "https://cnb.cool/liuxiaogang/promptcast.git", "HEAD:refs/heads/main");
      }
      console.log(`${release.tag_name}: CNB 附件、日志与更新清单已发布`);
    } finally { rmSync(temp, { recursive: true, force: true }); }
  }
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
