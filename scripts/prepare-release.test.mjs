import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

test("发布清单只包含 EXE 和 DMG，忽略 MSI 和应用压缩包", () => {
  const dir = mkdtempSync(join(tmpdir(), "promptcast-release-test-"));
  try {
    for (const name of ["assets", "src-tauri", "releases"]) mkdirSync(join(dir, name));
    for (const name of ["package.json", "src-tauri/tauri.conf.json"]) {
      writeFileSync(join(dir, name), JSON.stringify({ version: "0.0.10" }));
    }
    writeFileSync(join(dir, "src-tauri/Cargo.toml"), '[package]\nversion = "0.0.10"\n');
    writeFileSync(join(dir, "releases/v0.0.10.md"), "# 测试发布\n\n更新说明\n");
    const installers = ["PromptCast_0.0.10_universal.dmg", "PromptCast_0.0.10_x64-setup.exe"];
    for (const name of [...installers, "PromptCast_0.0.10_x64_en-US.msi", "PromptCast_universal.app.tar.gz"]) {
      writeFileSync(join(dir, "assets", name), name);
    }
    execFileSync(process.execPath, [fileURLToPath(new URL("./prepare-release.mjs", import.meta.url))], {
      cwd: dir, env: { ...process.env, GITHUB_REF_NAME: "v0.0.10" },
    });
    const manifest = JSON.parse(readFileSync(join(dir, "release.json"), "utf8"));
    assert.deepEqual(manifest.assets.map((a) => a.name).sort(), installers.sort());
    assert.equal(manifest.downloads["windows-x86_64"].name, installers.find((name) => name.endsWith(".exe")));
    assert.deepEqual(manifest.downloads["darwin-aarch64"], manifest.downloads["darwin-x86_64"]);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
