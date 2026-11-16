# 秒搜 / Miaosou

面向 **统信 UOS V20**（Debian 10 基座、x86_64）的本地文件搜索工具。Tauri v1 + Rust，完全离线运行。

[中文](#中文) · [English](#english)

---

## 中文

### 简介

秒搜对本地文件建立文件名索引，支持 Everything 风格查询语法与中文分词，毫秒级返回结果。索引与配置全部保留在本机，运行期不联网、不采集数据。

### 功能

- 文件名 / 路径搜索：AND、OR、NOT、括号、带引号短语、通配符，以及 `ext:`、`type:`、`size:`、`dm:` 等过滤器
- 中文分词与命中高亮，支持大小写敏感开关
- 实时索引：inotify 事件去抖后立即更新；额度不足时保留已覆盖目录的实时监控，未覆盖部分由周期对账兜底
- freedesktop 图标主题与缩略图缓存
- 复制文件到剪贴板（X11 属主模型），可在 DDE / GNOME 文件管理器中直接粘贴
- 在文件管理器中定位、调用系统默认程序打开
- 原生菜单与系统托盘、全局快捷键 `Ctrl+Shift+Space`、窗口几何持久化
- 单实例运行；关闭窗口后驻留托盘

### 系统要求

- 统信 UOS V20 / Debian 10 及以上，x86_64
- 依赖：`libwebkit2gtk-4.0-37`、`libgtk-3-0`、`libayatana-appindicator3-1`

### 安装

从 [Releases](https://github.com/RascalHzk/miaosou/releases) 下载 deb：

```bash
sudo dpkg -i miaosou_0.3.4_amd64.deb
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

Miaosou is a local file search tool for **UnionTech UOS V20** (Debian 10 base, x86_64), built with Tauri v1 and Rust. Everything runs offline: the index and configuration stay on the machine, and the app performs no telemetry.

### Features

- Filename/path search with Everything-style syntax: AND, OR, NOT, parentheses, quoted phrases, wildcards, plus `ext:`, `type:`, `size:`, `dm:` and related filters
- Chinese text segmentation and match highlighting, with a case-sensitivity toggle
- Real-time indexing via inotify (debounced); when the watch budget is insufficient, covered directories stay real-time and the remainder falls back to periodic reconciliation
- freedesktop icon themes and thumbnail cache
- Copy files to the clipboard (X11 owner model) for pasting into DDE/GNOME file managers
- Reveal in file manager, open with the system default application
- Native menu and system tray, global shortcut `Ctrl+Shift+Space`, persisted window geometry
- Single-instance; closing the window keeps the app in the tray

### Requirements

- UnionTech UOS V20 / Debian 10 or newer, x86_64
- Dependencies: `libwebkit2gtk-4.0-37`, `libgtk-3-0`, `libayatana-appindicator3-1`

### Install

Download the deb from [Releases](https://github.com/RascalHzk/miaosou/releases):

```bash
sudo dpkg -i miaosou_0.3.4_amd64.deb
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
