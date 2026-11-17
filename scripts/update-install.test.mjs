import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const script = fileURLToPath(new URL("../src-tauri/src/update/replace-macos.sh", import.meta.url));

for (const blocked of [false, true]) {
  test(blocked ? "更新遇到已有备份时中止，原应用和用户数据保持不变" : "替换应用后旧包完整保留，用户数据目录不变", () => {
    const root = mkdtempSync(join(tmpdir(), "promptcast-install-test-"));
    try {
      const app = join(root, "Prompt Cast.app");
      const stage = join(root, "stage");
      mkdirSync(app);
      mkdirSync(join(stage, "new.app"), { recursive: true });
      writeFileSync(join(app, "binary"), "old");
      writeFileSync(join(stage, "new.app", "binary"), "new");
      writeFileSync(join(root, "user.sqlite"), "user data");
      if (blocked) {
        mkdirSync(join(stage, "previous.app"));
        writeFileSync(join(stage, "previous.app", "binary"), "existing backup");
      }
      const result = spawnSync("/bin/sh", ["-c", '. "$1"; replace_app "$2" "$3"', "test", script, app, stage]);
      assert.equal(result.status, blocked ? 1 : 0, result.stderr.toString());
      assert.equal(readFileSync(join(app, "binary"), "utf8"), blocked ? "old" : "new");
      assert.equal(readFileSync(join(stage, "previous.app", "binary"), "utf8"), blocked ? "existing backup" : "old");
      assert.equal(readFileSync(join(root, "user.sqlite"), "utf8"), "user data");
    } finally { rmSync(root, { recursive: true, force: true }); }
  });
}

test("macOS 安装版本核对拒绝旧包，接受目标版本", () => {
  const root = mkdtempSync(join(tmpdir(), "promptcast-version-test-"));
  try {
    const app = join(root, "Prompt Cast.app");
    mkdirSync(join(app, "Contents"), { recursive: true });
    writeFileSync(join(app, "Contents", "Info.plist"), '<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleShortVersionString</key><string>0.1.2</string></dict></plist>');
    for (const version of ["0.1.1", "0.1.2"]) {
      const result = spawnSync("/bin/sh", ["-c", '. "$1"; verify_app_version "$2" "$3"', "test", script, app, version]);
      assert.equal(result.status, version === "0.1.2" ? 0 : 1, result.stderr.toString());
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});
