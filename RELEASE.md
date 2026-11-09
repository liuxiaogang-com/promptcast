# 发版与更新（PromptCast）

## 发布流程（从 0.0.9 起）

GitHub `liuxiaogang-com/promptcast` 构建、签名和发布正式 Release；CNB
`liuxiaogang/promptcast` 每 10 分钟主动同步公开产物。GitHub 不配置 CNB 密钥，
CNB 使用自身流水线的临时 `CNB_TOKEN`。CNB 仓库必须公开，客户端匿名读取日志和下载安装包。

客户端固定入口（HTTPS，可配置 302，建议 `Cache-Control: no-store`）：

| 用途 | 固定入口 | 当前目标 |
|---|---|---|
| 检查更新 | `https://go.liuxiaogang.com/promptcast-update` | `https://cnb.cool/liuxiaogang/promptcast/-/git/raw/main/update.json` |
| 版本日志 | `https://go.liuxiaogang.com/promptcast-changelog` | `https://cnb.cool/liuxiaogang/promptcast/-/git/raw/main/changelog.json` |

`changelog.json` 使用 `{ "releases": [{ "version", "title", "notes", "pub_date" }] }`，
按版本降序保留历史。客户端查看历史不触发更新检查，弹窗按版本号取日志；日志请求失败不阻止安装。
`update.json` 仍保留旧格式和 `title/notes`，兼容旧版解析；新版不依赖这两个字段来加载完整日志。

后续版本只发布 Windows NSIS `.exe` 和 macOS 通用 `.dmg` 两种安装包，
不再构建 MSI 或额外分发 `.app.tar.gz`。流水线直接调用 Tauri CLI，按平台指定
`--bundles nsis` / `--bundles dmg`，只上传对应安装文件。`release.json` 仍作为自动同步元数据保留。
已发布的 0.0.9 附件及历史索引保持不变，CNB 同步器继续兼容历史附件格式。

每次发布：

1. 同步修改 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json` 的版本号。
2. 新建 `releases/v版本.md`，第一行一级标题，其余为 Markdown 日志。
3. 提交到 GitHub main，再创建并推送 `v版本` tag；也将代码提交同步到 CNB main（不得强推）。
4. GitHub 两平台构建成功后生成 `release.json`（附件名称、大小、SHA256、版本日志），
   公开 Release 后才把元数据写入 main 的 `release-index.json`。CNB 读取静态索引，
   不调用 GitHub 匿名 Release API，避免共享出口的 API 额度限制。
5. CNB 定时任务下载并校验同一份安装包，上传 Release，匿名下载复核后，提交日志与更新清单。
   两个 JSON 在同一个提交中发布，清单不会早于日志和附件可用。

CNB 分支页提供「同步 GitHub 发布」按钮，可立即同步。定时与手动同步共用流水线锁；
已同步版本跳过，部分失败可以重试，不覆盖同名的不同内容附件，不回退更新清单。
CNB 生成的提交应正常合并回本地再推送后续代码，避免非快进冲突。

## 旧版迁移

0.0.8 及更早的已安装客户端仍读取
`https://raw.githubusercontent.com/jianjimi/promptcast/main/update.json`，
不能仅通过修改新仓库让旧版自动换入口。0.0.9 的 CNB 附件和清单就绪后，
将 CNB 的 `update.json` 复制到旧 GitHub 仓库同一路径，即可引导旧版安装过渡版本。
旧入口需保持公开；没有更新的客户端在入口关闭后需要手动安装。
旧版继续展示清单内嵌日志；新版使用固定日志入口，无需按客户端版本配置不同重定向。

## 构建提速：main 缓存预热

GitHub Actions 缓存按 git ref 隔离 —— tag 构建彼此看不到缓存，但都能读 **main** 的缓存。
`.github/workflows/warm-cache.yml` 在 `src-tauri/**` 或依赖变动时于 main 上跑一次 `tauri build --no-bundle`，
把 ~500 个依赖的编译产物预热进 main 作用域（与 release.yml 共用 `shared-key: tauri-release`）。
之后发版 tag 构建命中缓存：冷构建 ~10 分钟 → 热构建约 2-4 分钟。公开仓库 Actions 免费。

## macOS 代码签名（自签名证书）

### 为什么要签名
macOS 的辅助功能(TCC)授权按 App 的**代码签名身份**记。ad-hoc 未签名的包每次构建 cdhash 都变，
更新替换后系统当成"另一个 App"，授权作废、需重授。用**同一张证书**签名后，签名身份（指定要求 DR）
跨版本稳定，更新后授权保留。自签名非 Apple 公证，Gatekeeper 首次打开仍需右键「打开」，但不影响授权保留。

### 已有的配置
- **证书**：自签名代码签名证书，CN=`PromptCast Self-Signed`，有效期 100 年（`extendedKeyUsage=codeSigning`）。
- **GitHub Secrets**（已用 `gh secret set` 配在 `liuxiaogang-com/promptcast`）：
  - `APPLE_CERTIFICATE`：`.p12` 的 base64
  - `APPLE_CERTIFICATE_PASSWORD`：`.p12` 密码
  - `APPLE_SIGNING_IDENTITY`：`PromptCast Self-Signed`
- **CI**：`release.yml` 的 “Setup macOS signing keychain” 步导入 `.p12` 到临时钥匙串、
  `sudo security add-trusted-cert ... -p codeSign` 信任为代码签名根（自签名必须这步，否则 codesign 报
  no identity found）；构建步通过环境变量 `APPLE_SIGNING_IDENTITY` 让 tauri 用该身份签名。Windows 忽略这些。

GitHub 中的 PKCS12 容器使用 `PBE-SHA1-3DES` 和 SHA-1 MAC，以兼容 OpenSSL 3 与 macOS
钥匙串。旧 RC2 容器会被 OpenSSL 3 拒绝，AES/SHA-256 容器可能被钥匙串拒绝。
重新封装时复用原 `cert.pem` 和 `key.pem`，不要重新生成证书；封装算法不改变签名身份。

### 证书文件与备份（重要）
私钥证书在本机 **`~/Desktop/promptcast-signing/`**（不在 git 里）：
- `promptcast-codesign.p12`（含私钥）、`p12-password.txt`（密码）。
**务必备份到安全处**（云盘/密码管理器）。丢失只能重新生成新证书，等于换了身份、需再重授一次。

### 验证某个发版是否签名成功
```bash
gh release download vX.Y.Z --repo liuxiaogang-com/promptcast --pattern "*universal.dmg" -D /tmp/v && \
hdiutil attach /tmp/v/*.dmg -nobrowse -mountpoint /tmp/vm >/dev/null && \
codesign -dvv /tmp/vm/*.app 2>&1 | grep Authority ; hdiutil detach /tmp/vm >/dev/null
# 期望看到：Authority=PromptCast Self-Signed
```

### 重新生成 / 轮换证书（丢失或过期时）
```bash
DIR=~/Desktop/promptcast-signing; mkdir -p "$DIR"; cd "$DIR"
PASS=$(openssl rand -hex 16); echo "$PASS" > p12-password.txt
openssl req -x509 -newkey rsa:2048 -keyout key.pem -out cert.pem -days 36500 -nodes \
  -subj "/CN=PromptCast Self-Signed/O=PromptCast/C=CN" \
  -addext "basicConstraints=critical,CA:FALSE" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=critical,codeSigning"
openssl pkcs12 -export -inkey key.pem -in cert.pem -out promptcast-codesign.p12 \
  -certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES -macalg sha1 \
  -name "PromptCast Self-Signed" -passout pass:"$PASS"
# 重新写入 Secret：
base64 -i promptcast-codesign.p12 | gh secret set APPLE_CERTIFICATE --repo liuxiaogang-com/promptcast
printf '%s' "$PASS" | gh secret set APPLE_CERTIFICATE_PASSWORD --repo liuxiaogang-com/promptcast
printf '%s' "PromptCast Self-Signed" | gh secret set APPLE_SIGNING_IDENTITY --repo liuxiaogang-com/promptcast
```
> 换证书 = 新身份，下一次更新会再重授一次辅助功能，之后恢复保留。有效期到期前签新版会失败，
> 届时按上面重新生成即可（设了 100 年，正常不会遇到）。

### 一次性的过渡（0.0.6 未签名 → 0.0.7 签名）
身份从"无签名"变"有签名"，**0.0.6→0.0.7 这一次仍需重授一次**（移除系统设置里旧 PromptCast 条目、
重开触发授权、再重开）。从 0.0.7 起（同证书签名）授权一直保留。

## 本地端到端演练（不依赖外网 / 不发版）

清单地址虽预制，但后端保留了「DB 覆盖」后门（UI 不暴露）：DB 的 `update_manifest_url` 非空时优先用它。
更新器放行 `http://localhost` / `127.0.0.1` 明文，可本机全链路测：

```bash
# 1) 本机静态服务托管 update.json + 安装包
cd src-tauri/target/release/bundle && python3 -m http.server 8000
# 2) 写一个 update.json 放到该目录，version 调高（如 9.9.9），url 指向 http://localhost:8000/dmg/<dmg>
# 3) 临时设置后门地址（开发期可在前端临时调用，或直接改 settings 表 update_manifest_url）
#    → 启动/手动检查 → 应弹窗（含 立即更新/今天忽略/跳过当前版本）→ 下载 → open 挂载 dmg
```

## 注意点

- 自建更新器（托管 JSON 清单），非 Tauri 官方 updater，**无需** minisign 签名。
- 下载链接强制 https（防 MITM 注入安装器）；https→http 降级跳转被拒。
- macOS 通用包：`update.json` 里 `darwin-aarch64` 与 `darwin-x86_64` 指向同一 `*_universal.dmg`。
- macOS 包用自签名证书签名（非 Apple 公证），从别处（浏览器）下载首次打开仍需右键「打开」或
  `xattr -dr com.apple.quarantine <App>`；应用内更新走脚本替换并去隔离，relaunch 不受 Gatekeeper 阻拦。详见上文「macOS 代码签名」。
- keyring 在 macOS 走 Keychain，首次同步登录可能弹钥匙串授权，正常。
- 国内若 `raw.githubusercontent.com` 访问不稳，后续把清单与安装包镜像到 CNB 公开仓库，改 `DEFAULT_MANIFEST_URL` 重新发版即可。
