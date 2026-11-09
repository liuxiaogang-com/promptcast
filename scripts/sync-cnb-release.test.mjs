import { test } from "node:test";
import assert from "node:assert/strict";
import { validateManifest, publication } from "./sync-cnb-release.mjs";

const asset = { name: "PromptCast_0.0.9_universal.dmg", size: 10, sha256: "a".repeat(64) };
const manifest = {
  version: "0.0.9", title: "新版", notes: "更新说明", pub_date: "2026-11-09",
  assets: [asset], downloads: Object.fromEntries(
    ["darwin-aarch64", "darwin-x86_64", "windows-x86_64"].map((p) => [p, { ...asset }]),
  ),
};
test("缺少附件或摘要不一致时拒绝发布", () => {
  assert.throws(() => validateManifest(manifest, { tag_name: "v0.0.9", assets: [] }), /未齐全/);
  const invalid = structuredClone(manifest);
  invalid.downloads["windows-x86_64"].sha256 = "b".repeat(64);
  assert.throws(() => validateManifest(invalid, { tag_name: "v0.0.9", assets: [asset] }), /缺少平台/);
});
test("日志按版本保留、重试不重复，旧版任务不能回退更新入口", () => {
  const latest = { version: "0.0.10", title: "新版本" };
  const first = publication(manifest, { releases: [] }, latest);
  const retry = publication(manifest, first.changelog, latest);
  assert.equal(retry.changelog.releases.length, 1);
  assert.deepEqual(retry.update, latest);
  const published = publication(manifest, { releases: [] }, null);
  assert.equal(published.update.notes, manifest.notes);
  assert.match(published.update.downloads["darwin-aarch64"].url, /^https:\/\/cnb\.cool\//);
});
