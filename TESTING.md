# TESTING

本仓有两套可运行单元（内核 crate 工作区、Linux 壳独立工作区）和一个前端项目，测试分层如下。

## 1. 内核 crate（Rust 工作区）

```bash
cargo test --workspace          # 全部内核 crate
cargo test -p search-cache      # 只跑索引/查询
cargo test -p cardinal-syntax   # 查询语法解析
```

- 覆盖索引构建、持久化、查询语法、分词、取消令牌、slab 等平台无关逻辑。
- 文件图标 / 实时监控的 Linux 实现在 `linux/src-tauri/src/icons.rs` 与 `watcher.rs`。

## 2. Linux 壳（`linux/src-tauri`，独立工作区）

```bash
cd linux/src-tauri
cargo test --features custom-protocol
cargo test --features custom-protocol -- --ignored   # 真实文件系统事件
```

- 普通测试覆盖：真实扫描 + 搜索、持久化往返（写盘 → 读回 → 查询）、增量更新（新增/删除）、
  排序、窗口状态恢复、图标名映射、配置规范化、剪贴板路径过滤。
- `--ignored` 需要真实 inotify/kqueue：`watcher::tests::watch_applies_new_files` 在临时目录里
  建索引、启动监控、写入新文件，并断言它能被搜索到；`watcher::tests::notify_backend_smoke`
  只验证当前平台的 notify 后端能投递事件（排障用）。

## 3. 前端（`linux/ui`）

```bash
cd linux/ui
npm ci
npm run typecheck     # tsc --noEmit
npm run lint          # ESLint（flat config）
npm test              # vitest
npm run format:check  # Prettier
npm run build         # 产出 dist/，会被 Tauri 编进二进制
```

## 4. Debian 10 构建闸门（目标基线）

```bash
STAGE=check  ./linux/scripts/build-debian10.sh   # 编译检查（默认，最快暴露问题）
STAGE=test   ./linux/scripts/build-debian10.sh   # 单元测试 + 真实 inotify 事件流
STAGE=build  ./linux/scripts/build-debian10.sh   # release 二进制
STAGE=bench  ./linux/scripts/build-debian10.sh   # release 性能基准（默认合成 20 万文件）
STAGE=bundle ./linux/scripts/build-debian10.sh   # 额外打 deb
```

- 在 Debian 10（glibc 2.28，与 UOS V20 对齐）容器里编译，验证 webkit2gtk 4.0 / libsoup2 兼容性。
- `FRESH=1` 丢弃缓存卷重建；产物在 `linux/dist/`（不入库）。

### 性能基准

```bash
STAGE=bench ./linux/scripts/build-debian10.sh
# 或直接在目标机/容器里：
cargo test --release --features custom-protocol perf_benchmark -- --ignored --nocapture
```

- `MIAOSOU_BENCH_FILES`（默认 200000）、`MIAOSOU_BENCH_DIRS`（默认 500）、
  `MIAOSOU_BENCH_QUERIES`（默认 200）、`MIAOSOU_BENCH_KEEP=1`（保留合成目录）。
- 输出 `BENCH ...` 行：建索引耗时/吞吐、内存（VmRSS/VmHWM）、查询 p50/p95/max、
  落盘耗时与索引体积。

## 5. 目标机验收（UOS V20 / x86_64）

```bash
sudo dpkg -i linux/dist/miaosou_0.3.4_amd64.deb
MIAOSOU_SELFTEST=report miaosou          # 索引就绪后自动查询一次并打印命中路径
MIAOSOU_TRACE=1 miaosou                  # 打印监控原始事件、增量批次与查询耗时
MIAOSOU_WATCH_ROOT=/path/to/dir miaosou  # 用指定目录代替 $HOME 做小范围验证
MIAOSOU_RESCAN_INTERVAL_SECS=60 miaosou  # 监控降级时的全量对账间隔
```

人工检查项（无法在无图形容器里自动化）：

- 托盘/原生菜单外观与中文文案；关闭窗口后进程常驻、托盘可唤回。
- 搜索 → 选中文件 → 右键菜单（打开/定位/复制路径/复制文件）→ 在 DDE 文件管理器里粘贴。
- 预览策略：**不做内置预览**；双击、Ctrl+O、右键「打开」应调起系统默认程序（无 Quick Look 入口/空格键行为）。
- 部署前置：确认 `/proc/sys/fs/inotify/max_user_watches` 已按 README「部署」章节调大，
  并观察 `MIAOSOU_TRACE=1` 日志：应看到「实时监控已启动」或「实时监控覆盖 N 个目录（预算 …，未覆盖全部）」；
  只有完全无法监控时才会看到「降级为每 N 秒全量对账」。
- 定位命令：Windows 键/菜单「在文件管理器中显示」应拉起 dde-file-manager 并选中文件
  （若 `--show` 参数在目标版本不生效，会回退到打开父目录，同时 stderr 有尝试记录）。
- 原始分辨率与 125%/150% 缩放下的列表、图标与虚拟滚动。
- 全局快捷键 `Ctrl+Shift+Space` 唤起窗口（X11；Wayland 下受限）。
- 首次索引时 `$HOME` 的扫描耗时/内存（大数据量机型），以及 inotify 额度不足时的降级日志。

## 6. CI

| 工作流 | 内容 |
| --- | --- |
| `.github/workflows/rust-ci.yml` | macOS 内核 crate：fmt / clippy / check / test；Ubuntu 上的 Linux 壳：fmt / clippy / 单测 / 真实事件流；`search-cache` 大语料 bench（push main 或 `perf:` PR） |
| `.github/workflows/frontend-ci.yml` | `linux/ui`：Prettier / ESLint / `tsc --noEmit` / vitest / `vite build` |
| `.github/workflows/codeql.yml` | JavaScript/TypeScript 静态安全扫描 |
