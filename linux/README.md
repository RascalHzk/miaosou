# 秒搜 / Miaosou 信创移植层（linux/）

本目录是 **秒搜 / Miaosou** 的 **Tauri v1** 桌面壳，
目标是统信 UOS V20 / x86_64（Debian 10 基座，webkit2gtk 4.0）。

### 命名与标识符

| 用途 | 取值 | 落地位置 |
|---|---|---|
| 产品名（中文） | `秒搜` | 窗口标题、菜单、托盘、`Name[zh_CN]` |
| 产品名（英文/包名） | `Miaosou` / `miaosou` | deb 包名、`/usr/bin/miaosou`、日志前缀 |
| identifier | `cn.miaosou.app` | `tauri.conf.json`（仅打包与 GTK/D-Bus 身份） |
| 用户数据目录 | `$XDG_CONFIG_HOME/miaosou/` | `index.db` + `.window-state.json`（**与 identifier 解耦**） |
| 环境变量 | `MIAOSOU_*` | 见下方「验收辅助环境变量」 |

品牌字面量集中在 `src-tauri/src/identity.rs`；**内部代码身份**（`cardinal-sdk` / `cardinal-syntax` 等 vendored crate 名、
索引 slab 格式）刻意保持不变，以免打断与上游的 cherry-pick。

---

## 当前状态（P0）

| 项 | 状态 |
|---|---|
| Tauri v1 壳骨架（18 个与上游同名的 IPC 命令） | ✅ 已完成 |
| 模块划分对齐上游（main / commands / window_controls / window_state / lifecycle / search_activity） | ✅ 已完成 |
| 窗口控制（显示/隐藏/三态切换/前台判定） | ✅ 已完成（v1 版 `window_controls.rs`） |
| 窗口状态持久化（替代 window-state 插件，格式与插件 v2 兼容） | ✅ 已完成（4 单测 + 双跑冒烟验证） |
| 原生菜单 + 系统托盘（Rust 侧，替代 v2 前端 JS 构建） | ✅ 已完成；**Linux 不支持预定义编辑/视图项，已改自定义菜单项**（0.2.1） |
| 真实 React UI 迁移（v2 → v1 API） | ✅ 已完成（tsc 通过，vite 产物已嵌入，`frontend_ready` 探针确认挂载） |
| PoC 前端（零依赖静态页） | ✅ 已完成，**P1 已被上游真实 React UI 取代** |
| `cargo check` / `cargo build`（macOS） | ✅ 通过（tauri 1.8.3，链接 + 资源嵌入 OK） |
| 本机冒烟运行（窗口 + 全局快捷键注册） | ✅ 通过（8s 存活，无 panic） |
| Debian 10 容器内 `cargo check` / `cargo build`（webkit 4.0 链接） | ✅ 通过（产物 8.2MB x86-64，GLIBC ≤ 2.28） |
| Debian 10 容器内 `deb` 打包 | ✅ 通过（**4.1MB / miaosou 0.4.0**，含 UI + 索引 + 实时监控 + 图标/缩略图/剪贴板 + AppStream 元数据） |
| UOS V20 实机出图 / 中文输入 / 完整链路 | ⬜ 待验证 |
| 真实索引（fswalk + search-cache + slab-mmap 持久化） | ✅ 已完成（12 单测含持久化往返与旧数据迁移，运行时自检通过） |
| inotify 实时监控（去抖/预算保护/降级对账） | ✅ 已完成（真实事件流测试：macOS + Debian 10） |
| freedesktop 图标主题 + 缩略图缓存 | ✅ 已完成（图片/视频/PDF 走 `~/.cache/thumbnails`，未命中回退图标） |
| 文件管理器定位（DDE/GNOME/XFCE/KDE 候选 + 兜底） | ✅ 代码 + 单测；DDE 真机参数确认待 UOS 验收 |
| 性能基准（百万文件 / 4GB 机型目标） | ✅ 1M 文件：建索引 1.69s、查询 p50 6.1ms、索引 6.3MB；deb 4.2MB |
| 预览策略（P3 决策） | ✅ **不做内置预览**：双击 / Ctrl+O / 右键统一用系统默认程序打开 |
| 上架材料（AppStream + desktop + 图标 + copyright） | ✅ 已写入 deb（0.3.0） |
| 安全加固（CSP / freezePrototype / allowlist 最小化） | ✅ 已完成（见下「安全配置」） |
| 大文件拆分（工程债，公开 API 不变） | ✅ `search-cache/src/query.rs` 1688→1058、`cardinal-syntax/src/lib.rs` 1467→777 |

**P2 已完成真实索引接入**：`fswalk` 全量扫描 → `search-cache` 建索引 → 持久化到
`$XDG_CONFIG_HOME/miaosou/index.db`（zstd slab，格式与上游 macOS 版一致）→ 查询走真实内核
（Everything 语法、前缀/后缀/精确/globstar/正则、`ext:`/`file:`/`folder:` 等过滤）。
搜索、取节点元数据、远程排序均已是上游同款实现。

**验收辅助环境变量**（便于目标机排查）：

| 变量 | 作用 |
|---|---|
| `MIAOSOU_WATCH_ROOT=/path` | 强制覆盖监控根（不依赖前端 localStorage） |
| `MIAOSOU_SELFTEST=report` | 索引就绪后自动执行一次查询并打印命中路径 |
| `MIAOSOU_TRACE=1` | 打印每次查询的命中数与耗时 |

> 兼容：旧名 `CARDINAL_WATCH_ROOT` / `CARDINAL_SELFTEST` / `CARDINAL_TRACE` 仍可识别一个版本，
> 命中时会在 stderr 打一行弃用提示。新脚本请直接用 `MIAOSOU_*`。

> 说明：`MIAOSOU_WATCH_ROOT` 若指向默认忽略表里的路径（如 `/tmp/...`），Rust 侧会自动剔除
> 「根的祖先级」忽略规则，保证验收时能索引到内容。
>
> P2.5 已接入 inotify 实时监控（创建/修改/删除/重命名都会增量更新索引）。
> 若目标机目录数超过 inotify 额度，会自动降级为周期全量对账并提示 sysctl 修复命令。

---

## 目录

```
linux/
├── src-tauri/
│   ├── Cargo.toml          # tauri 1.8（features 必须与 allowlist 一致）
│   ├── tauri.conf.json     # v1 schema
│   ├── build.rs
│   ├── icons/              # 自有图标（source/make_icons.py 生成，Pillow 绘制）
│   ├── packaging/          # .desktop 模板 + /usr/share/doc/miaosou/copyright
│   └── src/
│       ├── main.rs             # 装配：builder / setup / RunEvent / 全局快捷键
│       ├── identity.rs         # 产品名 / identifier / 目录名 / MIAOSOU_* 环境变量（改名只改这里）
│       ├── paths.rs            # 用户数据目录 + 旧版本一次性迁移
│       ├── commands.rs         # 18 个 IPC 命令 + PoC 内存索引（P2 换真实管线）
│       ├── window_controls.rs  # 窗口显示/隐藏/三态切换/前台判定
│       ├── window_state.rs     # 窗口几何持久化（.window-state.json，兼容 v2 插件格式）
│       ├── native_ui.rs        # 原生菜单 + 系统托盘（替代 v2 前端 JS 构建）
│       ├── lifecycle.rs        # 应用生命周期（app_lifecycle_state 事件）
│       └── search_activity.rs  # 搜索活跃度（P2 空闲刷盘依据）
├── ui/                     # 上游 React 前端（v2 → v1 API 迁移后的版本）
│   ├── src/                #   React 组件 / hooks / i18n
│   └── dist/               #   vite 构建产物（会被编进二进制，gitignore）
└── scripts/build-debian10.sh
```

---

## 本机（macOS）快速验证

```bash
# 1) 先构建前端（v1 会把 dist/ 编进二进制）
cd linux/ui && npm ci && npm run build

# 2) 编译检查（只验证 v1 API 用法，不产出可运行包）
cd ../src-tauri
cargo check --features custom-protocol

# 2) 单元测试（窗口状态格式兼容 / 查询约束提取）
cargo test --features custom-protocol

# 3) 本机冒烟运行（需要 tauri CLI v1；也可 cargo run）
cargo install tauri-cli --version "^1.5" --locked
cargo tauri dev            # 或 cargo tauri build --debug
```

> 注意：仓库根的 `rust-toolchain.toml` 锁定 `nightly-2025-12-11`，`cargo` 首次运行会自动安装该 toolchain。

---

## Debian 10 构建（对齐 UOS V20 的 glibc 2.28 基线）

```bash
./linux/scripts/build-debian10.sh
```

前置：Docker。产物在 `linux/src-tauri/target/release/bundle/deb/`。

**不要**在高版本 Ubuntu/Debian 上构建后拿到 UOS 运行（会 `GLIBC_2.xx not found`）。

---

## 已验证的构建产物（P0 闸门全部通过）

| 文件 | 说明 |
|---|---|
| `linux/dist/miaosou_0.4.0_amd64.deb` | 4.2MB，真实 UI + 索引管线 + inotify 实时监控 + freedesktop 图标/缩略图 + 剪贴板 + 多桌面定位；sha256 `d6d06c0a…75d0`；`Depends: libayatana-appindicator3-1, libwebkit2gtk-4.0-37, libgtk-3-0` |
| `linux/dist/miaosou` | `ELF 64-bit x86-64`，动态链接，**GLIBC ≤ 2.28** |

> `linux/dist/` 是构建产物目录，已被 `.gitignore` 忽略；重新生成见下方构建命令。

在 UOS V20 上安装后：

```bash
sudo dpkg -i miaosou_0.4.0_amd64.deb
miaosou
# 若出现白屏/花屏（webkit 2.38 + 老内核）：
WEBKIT_DISABLE_DMABUF_RENDERER=1 miaosou
```

## UOS V20 实机检查清单（P0 门槛）

```bash
# 1) 确认 webkit 4.0 存在且版本
dpkg -l | grep webkit2gtk
# 期望：libwebkit2gtk-4.0-37  2.38.x（V20-1070 实测 2.38.6.0-deepin1）

# 2) glibc / 内核
ldd --version | head -1
uname -r

# 3) 安装 deb 后运行；若白屏/花屏，加环境变量
WEBKIT_DISABLE_DMABUF_RENDERER=1 miaosou
# 虚拟机或无 GPU：
WEBKIT_DISABLE_COMPOSITING_MODE=1 miaosou

# 4) 中文输入法：fcitx5 / ibus 输入中文到搜索框
# 5) 验证：搜索框输入 "report" → 有结果；双击打开；Ctrl+R 定位
```

---

## 部署（离线 / 信创环境）

**前置（推荐，一次性）**：inotify 额度决定了实时监控能否覆盖整个 `$HOME`。UOS 默认
`fs.inotify.max_user_watches=8192`，大目录树可能不够。不够时**已覆盖的目录仍然是实时更新**，
覆盖时按广度优先优先装浅层目录；只有未覆盖的目录由每 `MIAOSOU_RESCAN_INTERVAL_SECS`（默认 300s）的全量对账兜底；完全无法注册
watch 时才退化为纯周期对账。日志会打印「实时监控覆盖 N 个目录（预算 …，未覆盖全部）」：

```bash
# 立即生效
sudo sysctl -w fs.inotify.max_user_watches=524288
# 永久生效
echo 'fs.inotify.max_user_watches=524288' | sudo tee /etc/sysctl.d/99-miaosou.conf
```

**安装 / 升级 / 回滚**（离线包由运维统一分发）：

```bash
sudo dpkg -i miaosou_0.4.0_amd64.deb        # 安装或升级
sudo apt-get -f install                     # 若提示缺依赖（正常 UOS 桌面环境已自带）
miaosou                                     # 从应用菜单或终端启动

# 回滚：保留上一版 deb 即可；用户数据在 ~/.config/miaosou/（索引与窗口状态）
sudo dpkg -i miaosou_0.4.0_amd64.deb
```

- 索引数据库 `~/.config/miaosou/index.db` 与上游 zstd slab 格式一致；跨版本可复用，
  格式不兼容时会自动重建（首次启动稍慢）。
- 卸载：`sudo dpkg -r miaosou`（不会删除 `~/.config/miaosou/`，需要时手动清理）。
- 完全离线：运行期不联网；索引与配置全部在本地。

## 安全配置

- CSP：`default-src 'self'`、`script-src 'self'`、`img-src 'self' asset: data:`、
  `style-src 'self' 'unsafe-inline'`（React 行内样式所需）、`object-src/base-uri/form-action 'none'`。
- `freezePrototype: true`、`withGlobalTauri: false`（页面无法通过 `window.__TAURI__` 拿全局 API）。
- allowlist 最小化：仅 `window`（show/hide/setFocus）与 `globalShortcut`；**不含 shell**
  （打开文件走应用自己的 `open_path`/`open_in_finder`，不向 JS 暴露任意命令执行）。
- 不渲染不可信 HTML：预览功能已明确不做；文件名/路径均以纯文本渲染。

## 已知限制

- 原生菜单/托盘已实现（v1 `SystemTray` + appindicator）；菜单文案固定为简体中文（v1 无法运行时重建菜单）。
  Tauri v1 的预定义编辑/视图菜单项在 Linux 会被静默丢弃，因此「编辑」「视图」「最大化/还原」均为自定义项（0.2.1 修复）。
- 无文件拖出窗口（Tauri v1 无对应插件）。
- **不做内置预览**：统一用系统默认程序打开（双击 / Ctrl+O / 右键「打开」）；
  `quicklook` 三个命令仅为上游 API 兼容占位，Linux 前端不会调用，也没有 Quick Look 菜单项/空格键入口。
- 剪贴板文件复制走 X11 属主模型；Wayland 需启用 arboard 的 `wayland-data-control`（UOS V20 默认 X11）。
- 缩略图只读 freedesktop 缓存，不自行解码/生成；缓存里没有的文件回退图标主题。
- DDE `dde-file-manager --show` 的具体参数需在 UOS 真机确认（代码已按候选列表逐个尝试）。
- Wayland 下全局快捷键受限（v1 依赖 X11 grab）。
- `tag:` 标签过滤暂不支持（Linux 无 Finder 标签对应；`file-tags` 仅保留用于上游对照）。

---

## 编码约定（继承上游）

- Conventional Commits；`cargo fmt --all` / `cargo clippy`。
- 平台差异一律收口到 `src-tauri/src/platform/`（P2 建立），内核 crate 保持零 `cfg`。
- 命令名与上游 `commands.rs` 保持一一对应（本仓实现见 `linux/src-tauri/src/commands.rs`），方便前端机械迁移。
