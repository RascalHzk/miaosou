# 秒搜 / Miaosou

面向 **统信 UOS V20**（Debian 10 基座、x86_64）的本地文件搜索工具，支持 **Everything 风格查询语法**（AND/OR/NOT、通配符、`ext:`/`type:`/`size:`/`dm:` 等过滤器）与中文分词，毫秒级检索。Tauri v1 + Rust，完全离线运行。

[中文](#中文) · [English](#english)

---

## 中文

### 简介

秒搜对本地文件建立文件名索引，支持 Everything 风格查询语法与中文分词，毫秒级返回结果。索引与配置全部保留在本机，运行期不联网、不采集数据。

### 为什么做

信创环境（统信 UOS V20 / 国产化终端）通常缺少 Everything 一类「本地文件名即时搜索」工具：系统自带搜索依赖桌面索引，对海量目录和中文文件名的支持有限；商业工具又难以离线部署、审计和二次开发。

秒搜的目标很具体：

- **开箱即用**：安装 deb 后无需联网、无需账号，直接对本地文件建索引
- **保留 Everything 习惯**：AND/OR/NOT、通配符、`ext:`/`size:`/`dm:` 等语法尽量不改变用户习惯，降低迁移成本
- **面向低配与海量目录**：百万级文件仍尽量保持毫秒级查询；索引落盘，重启不必全量重扫
- **数据不出机器**：索引与配置只保存在本机，无遥测、无云依赖，适合内网/涉密环境
- **可维护、可审计**：Tauri v1 + Rust，兼容 UOS 的 webkit2gtk 4.0 与 glibc 2.28；MIT 许可，源码可查

不做的事：不做内置预览、不做标签体系（见「已知限制」），把复杂度留给系统默认程序与文件管理器。

### 设计取舍

- **Tauri v1 而不是 v2**：UOS V20 只有 webkit2gtk 4.0，v2 需要 4.1；v1 是当前能在这条基线上稳定运行的版本
- **默认忽略依赖/缓存/日志目录**：`node_modules`、`target`、`.git`、`~/.cache` 等对搜索没有价值，却会显著拖慢索引与实时监控；需要时可用「包含路径」加回来
- **实时监控采用部分覆盖**：inotify 额度不足时优先保留浅层目录的实时性，剩余目录由周期对账兜底，而不是整体退化为定时扫描
- **大小写不敏感查询不再逐条走 regex**：名字池预存小写名，字面量用 `memchr` 子串扫描；通配符仍保留 regex 语义

### 功能

- 文件名 / 路径搜索：AND、OR、NOT、括号、带引号短语、通配符，以及 `ext:`、`type:`、`size:`、`dm:` 等过滤器
- 中文分词与命中高亮，支持大小写敏感开关
- 实时索引：inotify 事件去抖后立即更新；额度不足时保留已覆盖目录的实时监控，未覆盖部分由周期对账兜底
- 默认忽略系统目录、缓存、回收站、系统日志与依赖/构建目录（`node_modules`、`target`、`.git`、`__pycache__` 等），可用「包含路径」覆盖
- freedesktop 图标主题与缩略图缓存
- 复制文件到剪贴板（X11 属主模型），可在 DDE / GNOME 文件管理器中直接粘贴
- 在文件管理器中定位、调用系统默认程序打开
- 原生菜单与系统托盘、全局快捷键 `Ctrl+Shift+Space`、窗口几何持久化
- 单实例运行；托盘开启时关闭窗口驻留托盘，托盘关闭时关闭窗口直接退出

### 性能与体积

> **性能速览（100 万文件，Debian 10 容器）**：建索引 **1.69s** · 查询 P50 **6.1ms** · 索引体积 **6.3MB** · 安装包仅 **4.2MB**。

以下为 Debian 10 容器（对齐 UOS V20：x86_64 / glibc 2.28）release 构建、合成目录树的实测；查询为随机文件名（大小写敏感路径），200 次取分位。

| 规模       | 建索引 | 查询 P50 / P95 | 索引体积 |
| ---------- | -----: | -------------: | -------: |
| 20 万文件  |  0.41s |  1.1ms / 1.2ms |    1.3MB |
| 100 万文件 |  1.69s |  6.1ms / 6.8ms |    6.3MB |

- 扫描吞吐约 **49–59 万文件/秒**（容器内合成树）
- **软件小巧**：deb 约 **4.2MB**，二进制约 **13MB**，不捆绑 Chromium/WebView 运行时（复用系统 WebKitGTK 4.0）
- 索引落盘只有 **1.3–6.3MB**；启动时 mmap 加载，无需重扫目录
- 没有索引变化时**不做周期全量扫描**；只有 inotify 覆盖不到的目录才由对账兜底
- 0.3.5 起大小写不敏感的短查询走**小写名字池 + `memchr`**，不再逐条编译 regex
- 默认忽略依赖/缓存/日志目录，进一步降低索引时间与索引体积

> 数字来自内置 benchmark（`STAGE=bench`），随 CPU、磁盘、文件类型与查询分布变化，用于量级参考。

### 系统要求

- 统信 UOS V20 / Debian 10 及以上，x86_64
- 依赖：`libwebkit2gtk-4.0-37`、`libgtk-3-0`、`libayatana-appindicator3-1`

### 安装

从 [Releases](https://github.com/RascalHzk/miaosou/releases) 下载 deb：

```bash
sudo dpkg -i miaosou_0.4.0_amd64.deb
# 如提示缺依赖
sudo apt-get -f install
miaosou
```

索引大目录时建议调大 inotify 额度（可选；不调也能用，未覆盖目录会自动降级为周期对账）：

```bash
sudo sysctl -w fs.inotify.max_user_watches=524288
# 永久生效
echo 'fs.inotify.max_user_watches=524288' | sudo tee /etc/sysctl.d/99-miaosou.conf
```

### 使用

- 启动后直接输入关键字；`Ctrl+L` 聚焦搜索框
- `↑` / `↓` 选择，`Enter` 打开，`Ctrl+O` 打开，`Ctrl+R` 在文件管理器中定位
- 右键结果可打开、定位、复制路径、复制文件；设置中用「×」或 `Esc` 关闭
- 设置里可调整监控根、忽略路径、包含路径、主题与语言

### 构建

仓库自带构建所需的内核 crates（vendored），无需额外拉取：

```bash
# 内核 crates（macOS 开发机同样可跑）
cargo test --workspace

# Linux 壳（Tauri v1）
cd linux/src-tauri
cargo test --features custom-protocol
cargo clippy --all-targets --features custom-protocol -- -D warnings

# 前端
cd linux/ui
npm ci && npm test && npm run build
```

Debian 10（对齐 UOS V20 的 glibc 2.28 基线）容器构建：

```bash
STAGE=test   ./linux/scripts/build-debian10.sh
STAGE=bundle ./linux/scripts/build-debian10.sh
# 产物：linux/dist/miaosou_<version>_amd64.deb
```

### 目录结构

```text
linux/
  ui/            React + TypeScript 前端（Vite）
  src-tauri/     Tauri v1 壳：命令层、索引、inotify、图标、菜单/托盘、窗口状态
  scripts/       Debian 10 容器构建脚本
cardinal-sdk/    vendored 事件类型
cardinal-syntax/ 查询语法解析
search-cache/    索引与查询内核
fswalk/          文件系统扫描
...              其余 vendored 内核 crates（MIT）
```

### 已知限制

- 不做内置预览：双击 / `Ctrl+O` / 右键统一用系统默认程序打开
- `tag:` 标签过滤暂不支持
- Wayland 下全局快捷键受 Tauri v1 限制（依赖 X11 grab）
- inotify 额度不足时，深层目录只能依赖周期对账

### 许可证

MIT。部分内核组件按 MIT 许可引入，版权与许可声明见 [NOTICE](NOTICE) 与 [LICENSE](LICENSE)。

---

## English

### Overview

Miaosou is a local file search tool for **UnionTech UOS V20** (Debian 10 base, x86_64) with **Everything-style query syntax** (AND/OR/NOT, wildcards, `ext:`/`type:`/`size:`/`dm:` filters) and Chinese segmentation. Built with Tauri v1 and Rust, fully offline: the index and configuration stay on the machine, and the app performs no telemetry.

### Why Miaosou

Xinchuang desktops (UnionTech UOS V20 and similar) generally lack an Everything-like instant local filename search: the built-in search relies on desktop indexing and struggles with very large trees and Chinese filenames, while commercial tools are hard to deploy offline, audit, and extend.

Miaosou is intentionally narrow:

- **Works offline out of the box**: install the deb, no account or network required; index local files immediately
- **Keeps Everything habits**: AND/OR/NOT, wildcards, `ext:`/`size:`/`dm:` and related syntax, so users do not have to relearn search
- **Built for low-end machines and large trees**: millions of files should still answer in milliseconds; the index is persisted, so restarts do not require a full rescan
- **Data stays on the machine**: index and configuration are local only, with no telemetry or cloud dependency; suitable for intranets and classified environments
- **Maintainable and auditable**: Tauri v1 + Rust, targeting UOS's webkit2gtk 4.0 and glibc 2.28; MIT licensed with source available

Non-goals: built-in preview and a tag system (see Known limitations); those are delegated to the system default applications and the file manager.

### Design choices

- **Tauri v1, not v2**: UOS V20 ships webkit2gtk 4.0 while v2 requires 4.1; v1 is the stable option on this baseline
- **Ignore dependencies, caches and logs by default**: `node_modules`, `target`, `.git`, `~/.cache` and similar directories add index and watcher cost without improving search; include paths can opt them back in
- **Partial real-time coverage**: when the inotify budget is insufficient, shallow directories keep real-time updates and the remainder falls back to periodic reconciliation instead of disabling the watcher entirely
- **Case-insensitive literals avoid per-query regex**: the name pool stores a lowercase copy and literal segments scan it with `memchr`; wildcard segments keep regex semantics

### Features

- Filename/path search with Everything-style syntax: AND, OR, NOT, parentheses, quoted phrases, wildcards, plus `ext:`, `type:`, `size:`, `dm:` and related filters
- Chinese text segmentation and match highlighting, with a case-sensitivity toggle
- Real-time indexing via inotify (debounced); when the watch budget is insufficient, covered directories stay real-time and the remainder falls back to periodic reconciliation
- Ignore system dirs, caches, trash, logs and dependency/build dirs (`node_modules`, `target`, `.git`, `__pycache__`, ...) by default; include paths override this
- freedesktop icon themes and thumbnail cache
- Copy files to the clipboard (X11 owner model) for pasting into DDE/GNOME file managers
- Reveal in file manager, open with the system default application
- Native menu and system tray, global shortcut `Ctrl+Shift+Space`, persisted window geometry
- Single-instance; with the tray enabled, closing the window keeps the app in the tray; with the tray disabled, closing it quits the app

### Performance and size

> **At a glance (1M files, Debian 10 container)**: index build **1.69s** · query P50 **6.1ms** · index size **6.3MB** · package only **4.2MB**.

Measured with a release build in a Debian 10 container (matching UOS V20: x86_64 / glibc 2.28) on a synthetic tree; queries are random filenames (case-sensitive path), percentiles over 200 runs.

| Scale      | Index build | Query P50 / P95 | Index size |
| ---------- | ----------: | --------------: | ---------: |
| 200k files |       0.41s |   1.1ms / 1.2ms |      1.3MB |
| 1M files   |       1.69s |   6.1ms / 6.8ms |      6.3MB |

- Scan throughput around **490k-590k files/s** (synthetic tree in the container)
- **Small footprint**: the deb is about **4.2MB** and the binary about **13MB**; no bundled Chromium/WebView runtime (uses the system WebKitGTK 4.0)
- The on-disk index is only **1.3-6.3MB**; it is mmap-loaded at startup, with no directory rescan
- **No periodic full scans** when nothing changes; only directories outside inotify coverage use reconciliation
- Since 0.3.5, case-insensitive short queries use the **lowercase name pool + `memchr`** instead of compiling a regex per query
- The default ignore list keeps dependency/cache/log directories out of the index, reducing build time and index size

> Numbers come from the built-in benchmark (`STAGE=bench`) and vary with CPU, disk, file types and query distribution; use them as an order-of-magnitude reference.

### Requirements

- UnionTech UOS V20 / Debian 10 or newer, x86_64
- Dependencies: `libwebkit2gtk-4.0-37`, `libgtk-3-0`, `libayatana-appindicator3-1`

### Install

Download the deb from [Releases](https://github.com/RascalHzk/miaosou/releases):

```bash
sudo dpkg -i miaosou_0.4.0_amd64.deb
sudo apt-get -f install   # only if dependencies are missing
miaosou
```

For large directory trees, raising the inotify limit is recommended but optional (uncovered directories fall back to periodic reconciliation):

```bash
sudo sysctl -w fs.inotify.max_user_watches=524288
# persist
echo 'fs.inotify.max_user_watches=524288' | sudo tee /etc/sysctl.d/99-miaosou.conf
```

### Usage

- Type a query right after launch; `Ctrl+L` focuses the search box
- `↑` / `↓` to select, `Enter` to open, `Ctrl+O` to open, `Ctrl+R` to reveal in the file manager
- Right-click a result to open, reveal, copy path or copy files; close preferences with `×` or `Esc`
- Preferences let you change the watch root, ignore/include paths, theme and language

### Build

The kernel crates required to build are vendored in this repository:

```bash
# kernel crates
cargo test --workspace

# Linux shell (Tauri v1)
cd linux/src-tauri
cargo test --features custom-protocol
cargo clippy --all-targets --features custom-protocol -- -D warnings

# frontend
cd linux/ui
npm ci && npm test && npm run build
```

Debian 10 container build (matches the UOS V20 glibc 2.28 baseline):

```bash
STAGE=test   ./linux/scripts/build-debian10.sh
STAGE=bundle ./linux/scripts/build-debian10.sh
# output: linux/dist/miaosou_<version>_amd64.deb
```

### Layout

```text
linux/
  ui/            React + TypeScript frontend (Vite)
  src-tauri/     Tauri v1 shell: commands, index, inotify, icons, menu/tray, window state
  scripts/       Debian 10 container build script
cardinal-sdk/    vendored event types
cardinal-syntax/ query syntax parser
search-cache/    index and query kernel
fswalk/          filesystem walker
...              remaining vendored kernel crates (MIT)
```

### Known limitations

- No built-in preview: double-click / `Ctrl+O` / context menu open the system default application
- `tag:` filtering is not supported yet
- Global shortcuts under Wayland are limited by Tauri v1 (X11 grab)
- When the inotify budget is insufficient, deep directories rely on periodic reconciliation

### License

MIT. Some kernel components are included under the MIT license; see [NOTICE](NOTICE) and [LICENSE](LICENSE) for copyright and license statements.
