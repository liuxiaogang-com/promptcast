// Release 公开后才更新静态索引，供 CNB 无令牌拉取；只提交这个文件。
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";

const manifest = JSON.parse(readFileSync("release.json", "utf8"));
const git = (...args) => execFileSync("git", args, { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] });
git("fetch", "origin", "main");
git("checkout", "-B", "publish-index", "origin/main");
const index = existsSync("release-index.json")
  ? JSON.parse(readFileSync("release-index.json", "utf8")) : { releases: [] };
index.releases = [...index.releases.filter((r) => r.version !== manifest.version), manifest]
  .sort((a, b) => b.version.localeCompare(a.version, "en", { numeric: true }));
writeFileSync("release-index.json", JSON.stringify(index, null, 2) + "\n");
git("add", "release-index.json");
if (git("diff", "--cached", "--name-only").trim()) {
  git("-c", "user.name=github-actions[bot]", "-c", "user.email=41898282+github-actions[bot]@users.noreply.github.com",
    "commit", "-m", `chore(release): 索引 v${manifest.version} [skip ci]`);
  git("push", "origin", "HEAD:main");
}
