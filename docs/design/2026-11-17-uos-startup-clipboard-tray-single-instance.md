# 秒搜 UOS 实机反馈修复设计

- 状态：Approved v1（已按决策实现于 0.4.0）
- 日期：2026-11-17
- 范围：启动卡顿、复制文件粘贴、托盘开关、单实例
- 适用版本：0.4.0（0.3.5 之后的下一个功能修复版）

---

## 1. 背景与目标

UOS V20 实机测试反馈了四个问题：

1. 首次打开卡顿，怀疑“打开就加载索引，UI 响应慢”；
2. 搜索结果里“复制文件”，到文件管理器或其他地方粘贴不生效；
3. 设置里关闭“显示托盘图标”后，托盘图标不消失（已确认是此语义，对应问题 A）；
4. 重复打开会启动多个进程，期望复用已有进程并唤起窗口。

本文只做设计，不改代码。目标是给出可直接实施的改造方案、接口、线程模型、失败处理和验收标准。

### 非目标

- 不重写 Tauri v1 壳，不升级到 Tauri v2；
- 不引入云端/遥测；
- 不改变搜索语法与索引格式；
- 不处理与此四项无关的 UI 细节。

---

## 2. 现状与根因

### 2.1 启动卡顿

当前启动链路：

```text
main.tsx 渲染 React
  → invoke('frontend_ready') 显示窗口
  → useAppPreferences effect 调用 invoke('start_logic')
      → 同步 command
      → apply_watch_config → IndexHandle::ensure_ready
          → 同步读持久化索引（zstd 解压 + 重建 NameIndex/NamePool）
          → restart_watcher → FsWatcher::start
              → 等待初始 watch 安装完成（最多 10s）
```

关键点：

- `start_logic` / `set_watch_config` 是**同步 Tauri command**，不会自动搬到后台线程；在 v1 的调用路径上会占用 invoke/GTK 主循环，窗口虽然已经显示，但事件循环被占住。
- `ensure_ready` 在同一个调用栈里完成“读缓存 + 构建名字池 + 启动 watcher（等待就绪）”。1M 文件时名字池是百万级条目，0.3.5 还多存了一份小写名；watcher 初始安装可能覆盖数万目录，等待时间不可控。
- 首次安装没有持久化缓存：`ensure_ready` 会起后台重建线程，主线程不等扫描，但 fswalk 使用 rayon 并行遍历，默认线程数等于 CPU 核数；UOS 低配机上会和 WebKit 渲染抢 CPU/磁盘，UI 仍会明显卡顿。
- 0.3.5 新增了“忽略规则升级 marker”，升级后第一次启动会强制全量重建一次，进一步放大了“首次打开慢”的体感。

结论：卡顿是**主线程阻塞** + **首扫资源竞争**叠加，不是 WebView 加载慢。

### 2.2 复制文件粘贴不生效

当前 Linux 剪贴板实现使用 `arboard 3.7` 的 `file_list`：

```rust
let result = clipboard.set().wait().file_list(&paths);
```

查 arboard 3.7.0 的 Linux 实现，X11 只声明 `text/uri-list`（Wayland 同样是 `text/uri-list`），没有 `x-special/gnome-copied-files`。

而 DDE（deepin 文件管理器）在复制文件时，剪贴板 TARGETS 包含 `x-special/gnome-copied-files`，内容格式为：

```text
copy
file:///path/to/a
file:///path/to/b
```

只提供 `text/uri-list` 时，文件管理器可能把它当作普通文本或忽略，导致“粘贴不生效”。参考：

- [Qt 对 X11 文件剪贴板的说明：Nautilus 需要 x-special/gnome-copied-files](https://stackoverflow.com/revisions/377b3838-3f11-4952-86cc-7d8f1f04bb9e/view-source)
- [deepin 下复制文件时 TARGETS 包含 x-special/gnome-copied-files](https://unix.stackexchange.com/revisions/5e403670-cce5-48bf-8809-6e31eb1e2485/view-source)
- [X11 文件剪贴板的两种常见格式](https://unix.stackexchange.com/revisions/1a06bc89-6317-4691-a949-cd6b4c1cbcc0/view-source)

`.wait()` 让进程保持 selection owner 是必要的，但不是根因；根因是 **MIME 种类不完整**。

### 2.3 托盘开关不生效

设置里的“显示托盘图标”最终调用：

```rust
#[tauri::command]
pub fn set_tray_activation_policy(app: AppHandle, enabled: bool) {
    // Linux 无 macOS 的 accessory/regular 激活策略；v1 也没有运行时移除托盘的 API。
    let _ = (app, enabled);
}
```

Linux 上是空实现。实际 Tauri 1.8.3 提供：

- `SystemTrayHandle::destroy()`：销毁托盘；
- `SystemTray::build(&app)`：运行时创建托盘；
- `Builder::on_system_tray_event` 是全局回调，销毁/重建后仍能收到菜单事件。

所以可以在运行时实现“图标消失/重新出现”。

### 2.4 重复启动多进程

仓库中没有 `single-instance` 插件、socket、锁文件或 D-Bus 名称检查。每次 `.desktop` 启动都会走完整 Tauri 初始化，产生新进程，并可能：

- 同时打开多个窗口；
- 多个进程同时读/写 `~/.config/miaosou/index.db`；
- 各自启动 watcher，重复消耗 inotify 额度。

`tauri-plugin-single-instance` 当前最新为 3.x（v2 线），v1 没有可直接使用的稳定插件，因此采用自建方案。

---

## 3. 总体设计

### 3.1 线程模型

```text
GTK/主线程
  ├─ 窗口事件、菜单/托盘事件
  ├─ 轻量 command（show/hide/tray toggle/clipboard 设置）
  └─ 不执行磁盘扫描、不等待 watcher

索引工作线程（每次构建一个）
  ├─ 读缓存 / 全量扫描
  ├─ 构建 SearchCache
  └─ 完成后整体替换索引（Mutex<Option<SearchCache>>）

watcher 线程
  ├─ 安装初始 watch（异步就绪）
  ├─ 收集事件 → 去抖 → 批量应用
  └─ 部分覆盖时周期对账

单实例监听线程
  └─ 接收第二次启动的 SHOW 请求 → run_on_main_thread(activate)
```

原则：**主线程只做 UI 和轻量状态变更**；索引加载/扫描、watcher 就绪等待都不允许占用主线程。

### 3.2 四项改动的依赖关系

```text
单实例（最先执行，早于 Tauri builder）
   └─ 保证后续启动/索引逻辑只有一个进程

启动异步化
   └─ 依赖索引线程模型；与单实例无耦合

剪贴板
   └─ 独立模块；依赖 GTK 主循环

托盘开关
   └─ 依赖托盘句柄状态；与启动异步化无耦合
```

---

## 4. 详细设计

## 4.1 启动卡顿

### 4.1.1 Command 异步化

`start_logic`、`set_watch_config` 改为 async command，并把阻塞逻辑放进 `spawn_blocking`：

```rust
#[tauri::command]
pub async fn start_logic(
    watch_root: String,
    ignore_paths: Vec<String>,
    include_paths: Option<Vec<String>>,
    app: AppHandle,
    state: State<'_, SharedIndex>,
) -> Result<(), String> {
    let index = state.inner().clone();
    let config = resolve_config(...)?; // 纯计算，轻量
    tauri::async_runtime::spawn_blocking(move || {
        index.ensure_ready(&app, config)
    })
    .await
    .map_err(|err| format!("启动索引任务失败: {err}"))?
}
```

注意：

- 不能把 `State<'_, _>` 移进 `'static` 闭包，需先 `clone()` 出 `Arc<IndexHandle>`；
- `resolve_config` 中的路径规范化（canonicalize）可能有磁盘 IO，也放到 blocking 闭包里；
- `set_watch_config` 同理；
- `trigger_rescan` 本身只 `force_rebuild`（起线程），可以保留同步，或一并 async 化保持一致性。

前端调用方式不变（仍是 `invoke('start_logic', ...)`），无需改 API。

### 4.1.2 启动阶段与状态

```text
t0  main() 启动
t1  Tauri setup：恢复窗口、注册快捷键、创建托盘
t2  frontend_ready → 显示窗口（首帧）
t3  React effect → start_logic（立即返回）
t4  后台：读缓存 / 重建索引
t5  后台：启动 watcher
t6  Ready：emit index_ready / status_bar_update
```

要求：

- t2 到 t3 之间窗口必须已可交互；
- t4/t5 不阻塞主线程；
- t6 之前 UI 显示加载态，搜索请求返回空/loading，而不是卡死。

现有 `frontend_ready` 已在 React 渲染后调用，顺序保持不变；异步化后不再需要额外的延迟 hack。如果实测首帧仍被 effect 抢占，可把 `start_logic` 调用放进 `requestAnimationFrame` 或 `setTimeout(0)`，让首帧先出。

### 4.1.3 watcher 就绪不阻塞调用方

当前 `FsWatcher::start` 内：

```rust
if ready_rx.recv_timeout(Duration::from_secs(10)).is_err() {
    eprintln!("等待文件监控就绪超时");
}
```

改造建议：

- 保留 readiness 握手，但把超时缩短到 **200~500ms**；
- 超时后不阻塞调用方，watcher 在后台继续安装；
- 由于 `ensure_ready` 已经在 blocking 线程里，这一步不再影响 UI；缩短超时是为了让 `Ready` 状态尽快到达。

### 4.1.4 首扫资源限制

两级措施：

1. **限制 rayon 全局线程数**（在 `main()` 中、任何扫描开始前设置）：

   ```rust
   let threads = std::env::var("MIAOSOU_INDEX_THREADS")
       .ok()
       .and_then(|v| v.parse().ok())
       .unwrap_or_else(|| {
           std::thread::available_parallelism()
               .map(|n| n.get().saturating_sub(1).clamp(1, 4))
               .unwrap_or(2)
       });
   let _ = rayon::ThreadPoolBuilder::new()
       .num_threads(threads)
       .thread_name(|i| format!("miaosou-index-{i}"))
       .build_global();
   ```

   说明：fswalk、search-cache 内容搜索都用全局 rayon；限制后搜索吞吐可能略降，但避免 UI 饥饿。默认最多 4 线程，低配机保留 1 个核给 UI/WebKit。

2. **best-effort 降优先级**：在索引构建线程启动时调用 `setpriority(PRIO_PROCESS, 0, 10)`（失败忽略），减少与桌面的 CPU 竞争。

3. **进度事件节流**：现有 `STATUS_EMIT_INTERVAL=200ms` 保持不变；首次扫描时不要额外高频 emit。

### 4.1.5 并发配置变更

异步化后可能出现“用户连续改配置，多个 worker 竞争”的情况。设计：

- `IndexHandle` 增加 `config_generation: AtomicU64`；
- 每次 `ensure_ready` 开始 `fetch_add(1)` 得到 `my_gen`；
- 读完缓存/扫描完成后，只有 `my_gen == current` 才安装索引并启动 watcher，否则丢弃结果；
- `is_building` 仍用于防止同一时刻重复全量扫描。

### 4.1.6 首次索引提示

首次安装或 marker 触发重建时：

- UI 显示“正在建立索引（首次启动会稍慢）”；
- 进度继续用 `status_bar_update`；
- 不因为“没有结果”显示空态错误。

---

## 4.2 剪贴板（复制文件）

### 4.2.1 技术选型

使用 **GTK clipboard**，不使用 arboard 的 Linux file_list。

理由：

- Tauri v1 的依赖树里已经有 `gtk 0.15.5`，版本匹配；
- GTK 在主循环里持有 selection，进程存活期间持续可粘贴；
- 可同时声明多个 target（`text/uri-list`、`x-special/gnome-copied-files`、`text/plain`）；
- X11/Wayland 都由 GTK 处理。

备选：`x11rb` 自建 X11 owner 线程（仅 X11，Wayland 还要 data-control），复杂度更高，不作为首选。

### 4.2.2 数据格式

对 `paths: Vec<PathBuf>`：

```text
text/uri-list:
file:///home/u/a.txt\r\nfile:///home/u/b.txt\r\n

x-special/gnome-copied-files:
copy\nfile:///home/u/a.txt\nfile:///home/u/b.txt

text/plain:
/home/u/a.txt\n/home/u/b.txt
```

URI 使用现有 `paths::file_uri`（百分号编码，已处理空格/中文）。

### 4.2.3 实现方式

新增 `linux/src-tauri/src/clipboard.rs`（Linux 专属）：

```rust
pub fn copy_files(app: &AppHandle, paths: Vec<PathBuf>) -> Result<(), String> {
    let payload = Arc::new(ClipboardPayload::new(paths)?);
    app.run_on_main_thread(move || {
        let clipboard = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
        let targets = [
            gtk::TargetEntry::new("text/uri-list", gtk::TargetFlags::empty(), 0),
            gtk::TargetEntry::new("x-special/gnome-copied-files", gtk::TargetFlags::empty(), 1),
            gtk::TargetEntry::new("text/plain", gtk::TargetFlags::empty(), 2),
        ];
        let data = payload.clone();
        if let Err(err) = clipboard.set_with_data(&targets, move |_, selection| {
            let bytes = match selection.target().as_str() {
                "text/uri-list" => data.uri_list.as_bytes(),
                "x-special/gnome-copied-files" => data.gnome_copied.as_bytes(),
                _ => data.plain.as_bytes(),
            };
            selection.set(selection.target(), 8, bytes);
        }) {
            eprintln!("[miaosou] 写入剪贴板失败: {err}");
        }
    }).map_err(|err| format!("切换到 GTK 主线程失败: {err}"))?;
    Ok(())
}
```

要点：

- GTK API 必须在主线程调用，`run_on_main_thread` 是唯一入口；
- `set_with_data` 的回调持有 `Arc` 数据，GTK 在粘贴时回调；不要使用 `arboard` 的 `.wait()` 线程；
- 命令仍叫 `copy_files_to_clipboard`，payload 不变，前端无感；
- 失败要返回错误并写日志（当前命令是 `() -> ()`，建议改为 `Result<(), String>`，前端 `.catch` 提示）。

### 4.2.4 未使用 Wayland 时的行为

UOS 默认会话可能是 X11；GTK 在 Wayland 下走 Wayland data source，同样支持上述 MIME。若目标环境是纯 Wayland 且 DDE 仍不识别，需要再做一次 TARGETS 实测，不改架构。

### 4.2.5 验证

```bash
# 在 UOS 上复制后
xclip -selection clipboard -t TARGETS -o
xclip -selection clipboard -t x-special/gnome-copied-files -o
xclip -selection clipboard -t text/uri-list -o
```

验收：TARGETS 同时包含两种文件 MIME；DDE 文件管理器可粘贴；中文/空格路径可粘贴；多选可粘贴。

---

## 4.3 托盘开关（问题 A）

### 4.3.1 目标行为

设置“显示托盘图标”：

- 打开 → 托盘出现（若不存在则创建）；
- 关闭 → 托盘立即消失；
- 关闭后再次打开 → 托盘重新出现，菜单“打开/退出”仍可用；
- 设置值持久化（当前 localStorage），重启后保持。

### 4.3.2 状态管理

```rust
pub struct TrayState {
    handle: Mutex<Option<tauri::SystemTrayHandle<tauri::Wry>>>,
}
```

- 启动时：`Builder::system_tray(build_tray())` 创建托盘；在 `setup` 里用 `app.tray_handle()` 取句柄存入 `TrayState`（或统一在 `setup` 里 `build_tray().build(app)` 创建，避免双入口）。
- `set_tray_activation_policy(app, enabled)`：
  - `true`：若 `None`，`native_ui::build_tray().build(&app)`，存句柄；
  - `false`：若 `Some(handle)`，`handle.destroy()`，置 `None`；
- 命令返回 `Result<(), String>`，失败写日志并回传前端。

### 4.3.3 关闭窗口语义

当托盘被关闭时，“关闭窗口 = 隐藏到后台”会让应用变成**无托盘、无窗口**的不可见进程，只能靠全局快捷键找回。设计决策（建议）：

- 托盘存在：`CloseRequested` → `prevent_close + hide`（保持现状）；
- 托盘不存在：`CloseRequested` → 正常关闭/退出（不 `prevent_close`）；
- 或者关闭托盘开关时，UI 明确提示“关闭托盘后，关闭窗口将退出应用”。

实现上在 `CloseRequested` 里查询 `TrayState` 是否有句柄。

### 4.3.4 已知取舍

- 启动时托盘先按默认创建，前端 effect 读到“关闭”偏好后再销毁，可能出现短暂闪现。彻底消除需要把偏好存到 Rust 配置并在 `setup` 决定是否创建，属于后续优化项，不阻塞本次修复。
- Tauri v1 Linux 的 `set_tooltip` 不支持；`build_tray` 的 tooltip 在 Linux 无效果，不影响功能。

### 4.3.5 验证

- 设置中切换开关 2 次，观察托盘图标出现/消失/再出现；
- 重启后偏好保持；
- 托盘菜单“打开 秒搜/退出 秒搜”仍工作；
- 托盘关闭时点窗口关闭按钮，应用按 4.3.3 的决策退出，不残留不可见进程。

---

## 4.4 单实例（Unix socket + flock）

### 4.4.1 目标

- 第二次启动不创建新进程，唤起已有窗口；
- 首实例隐藏/最小化时也能被唤起；
- 首实例仍在启动/加载时，第二次启动不崩溃、不重复写索引；
- 崩溃后残留 socket 不影响下次启动。

### 4.4.2 文件与权限

路径：

```text
$XDG_RUNTIME_DIR/miaosou/instance.lock
$XDG_RUNTIME_DIR/miaosou/instance.sock
```

`$XDG_RUNTIME_DIR` 不存在时降级：

```text
/tmp/miaosou-<uid>/instance.lock
/tmp/miaosou-<uid>/instance.sock
```

权限：

- 目录 `0700`；
- socket `0600`；
- lock 文件 `0600`；
- 仅同 UID 可访问。

### 4.4.3 选主与监听

```rust
pub enum Instance {
    Primary(InstanceGuard),
    Secondary,
}
```

启动流程（在 Tauri builder 之前）：

1. 创建目录；
2. 打开/创建 `instance.lock`；
3. `flock(fd, LOCK_EX | LOCK_NB)`：
   - 成功 → Primary：删除旧的 `instance.sock`，`UnixListener::bind`，spawn 监听线程；
   - 失败且 errno = `EWOULDBLOCK` → Secondary：连接 socket；
4. Primary 持有 `File`（lock fd）直到进程退出；`Drop` 时删除 socket。

依赖：`libc`（已有）调用 `flock`；无需新增 nix/fs2。

### 4.4.4 协议

极简文本协议（一行）：

```text
SHOW\n   → 首实例显示/聚焦窗口，返回 OK\n
PING\n   → 返回 PONG\n（诊断用）
```

监听线程：

```rust
for stream in listener.incoming() {
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    match line.trim() {
        "SHOW" => activate(app_handle), // run_on_main_thread
        "PING" => write "PONG\n",
        _ => {}
    }
}
```

### 4.4.5 首实例尚未就绪

第二次启动可能发生在首实例 Tauri 还没建好时：

- 监听线程在 `setup` 之后才拿到 `AppHandle`；
- 在拿到之前收到 `SHOW` → 设置 `pending_show = true`；
- `guard.attach(app_handle)` 时检查 `pending_show`，通过 `run_on_main_thread` 执行激活；
- 第二次启动连接 socket 后等待 `OK`，最多 1s；超时也退出 0，不重复启动。

### 4.4.6 激活窗口

首实例收到 `SHOW`：

```rust
app.run_on_main_thread(move || {
    window_controls::activate_main_window_impl(&app);
    // 可选：emit("single_instance_activate") 让前端聚焦搜索框
})?;
```

`activate_main_window_impl` 已处理 `show/unminimize/set_focus`。

### 4.4.7 失败模式

| 场景 | 处理 |
|---|---|
| 第二次启动，socket 连接失败 | 重试 3 次（50ms 间隔）；仍失败则退出 0，不启动新实例 |
| 上次崩溃残留 socket | Primary 拿到 flock 后先删除旧 socket 再 bind |
| 上次崩溃残留 lock | flock 随进程退出自动释放 |
| 不同用户同时启动 | 路径含 UID / XDG_RUNTIME_DIR，互不影响 |
| 首实例启动中收到 SHOW | 记录 pending，attach 后激活 |
| 首实例已退出但 socket 未删 | 新 Primary 删除后重建 |

### 4.4.8 与 DDE 的配合

`.desktop` 的 `Exec=miaosou` 不变；第二次点击菜单时：

- 新进程会在 Tauri 初始化前退出，启动很快；
- 首实例窗口被唤起；
- 不会出现第二个托盘/第二个 watcher。

### 4.4.9 验证

- 连续启动两次：`ps` 只一个 `miaosou`，第二次退出码 0；
- 首实例隐藏到托盘后第二次启动 → 窗口显示并聚焦；
- 首实例启动中立刻第二次启动 → 不崩溃、不重复索引；
- `kill -9` 首实例后再启动 → 正常成为 Primary；
- 不同用户/不同 `XDG_RUNTIME_DIR` 不互相干扰。

---

## 5. 代码与接口变更清单

### 5.1 新增

| 文件 | 内容 |
|---|---|
| `linux/src-tauri/src/single_instance.rs` | flock 选主、UnixListener、协议、pending show、guard |
| `linux/src-tauri/src/clipboard.rs` | GTK 剪贴板 payload 与 set_with_data |
| `docs/design/2026-11-17-...md` | 本文档 |

### 5.2 修改

| 文件 | 变更 |
|---|---|
| `main.rs` | 启动前调用 `single_instance::acquire()`；rayon 线程池限制；setup 里 attach app handle、初始化 tray state |
| `commands.rs` | `start_logic`/`set_watch_config` 异步 + spawn_blocking；`copy_files_to_clipboard` 改 Result；`set_tray_activation_policy` 实现 destroy/rebuild |
| `index.rs` | `ensure_ready` 在 blocking 线程执行；config generation；启动阶段日志 |
| `watcher.rs` | readiness 超时缩短；不阻塞调用方 |
| `native_ui.rs` | 抽出 `install_tray(app)`；托盘句柄存取 |
| `Cargo.toml` | Linux 专属 `gtk = "0.15"`；`libc`（已有） |
| 前端（可选） | 启动加载态文案；剪贴板失败提示；托盘关闭后的关闭行为提示 |

### 5.3 不变量

- 搜索命令 `search`、`get_nodes_info` 等 IPC 签名不变；
- 索引文件格式不变；
- `MIAOSOU_*` 环境变量不变；
- 版本号与 CHANGELOG 在同一实现 PR 中更新。

---

## 6. 测试计划

### 6.1 自动化

- `cargo test --features custom-protocol`：现有 33 个单测保持；
- 新增单测：
  - clipboard payload：URI 编码、`text/uri-list`/`gnome-copied-files` 内容、多文件、中文/空格；
  - single_instance：路径选择、协议解析、锁选主（可用 fork/子进程辅助）；
  - tray state：无 GUI 下只测状态机（可选）。
- 前端：现有 vitest 保持；若加 loading 状态，补 1~2 个组件测试；
- Debian 10 容器 `STAGE=test`、`STAGE=bundle` 必须通过。

### 6.2 UOS 实机

| 项 | 步骤 | 通过标准 |
|---|---|---|
| 冷启动 | 删除缓存后启动，秒表/日志 | 窗口 < 1s 可交互；不出现整窗卡死 |
| 热启动 | 有缓存启动 | 窗口立即响应；索引后台加载 |
| 复制粘贴 | 单选/多选/中文路径/空格路径 → DDE 粘贴 | 文件真正复制；TARGETS 含两种 MIME |
| 托盘开关 | 关→开→关 | 图标消失/出现；菜单仍可用；重启保持 |
| 关闭窗口 | 托盘开/关两种状态点 X | 托盘开→隐藏；托盘关→按设计退出 |
| 单实例 | 连续启动 2 次；隐藏后启动；启动中启动 | 始终一个进程；已有窗口被唤起 |
| 崩溃恢复 | `kill -9` 后再启动 | 正常启动，无残留 socket 阻塞 |
| 回归 | 搜索、实时更新、快捷键、定位 | 与 0.3.5 一致 |

### 6.3 性能验收

- 主线程阻塞：用启动阶段日志/`strace` 粗测，任何单次主线程阻塞 < 50ms；
- 1M 文件索引构建不慢于 0.3.5 的 1.69s（容器基线，允许 ±20%）；
- 首次打开不再因为 `start_logic` 出现秒级无响应。

---

## 7. 风险与对策

| 风险 | 影响 | 对策 |
|---|---|---|
| GTK clipboard 线程/生命周期错误 | 粘贴仍失败或崩溃 | 所有 GTK 操作走 `run_on_main_thread`；payload 用 `Arc`；失败回退 arboard `text/uri-list` |
| DDE 对 gnome-copied-files 格式挑剔 | 仍无法粘贴 | 先按 Nautilus 格式（`copy\n` + URI 行）；用 TARGETS/xclip 实测，再微调结尾 NUL |
| 托盘 destroy 在部分 appindicator 版本不稳定 | 图标残留/崩溃 | destroy 失败则保留并回传错误；必要时降级为“隐藏设置项” |
| 关闭托盘后关闭窗口导致应用不可见 | 用户找不到应用 | 采用 4.3.3：无托盘时关闭窗口即退出 |
| 单实例 socket 权限/路径异常 | 无法唤起或误判 | 目录 0700、socket 0600；连接失败重试；失败退出不重复启动 |
| 异步化引入配置竞争 | 旧结果覆盖新配置 | `config_generation` 校验；`is_building` 防重 |
| rayon 限线程影响搜索吞吐 | 搜索变慢 | 默认最多 4 线程；`MIAOSOU_INDEX_THREADS` 可调；实测 P95 回归 |

---

## 8. 实施顺序与工作量

| 顺序 | 任务 | 预估 |
|---|---|---|
| 1 | 单实例模块 + main 接入 + 测试 | 0.5~1 天 |
| 2 | 启动异步化 + 线程池限制 + 阶段日志 | 0.5 天 |
| 3 | GTK 剪贴板模块 + 命令改造 | 1 天 |
| 4 | 托盘 destroy/rebuild + 关闭语义 | 0.5 天 |
| 5 | UOS 实机验证 + 打包 | 0.5 天 |

版本：0.4.0；全部改动先在本地打包，UOS 验证通过后再推送 GitHub。

---

## 9. 已确认的决策（2026-11-17）

1. **托盘关闭后，窗口关闭 = 直接退出应用**。仅在偏好设置里“关闭托盘”时生效；托盘开启时保持原有逻辑（关闭窗口只隐藏到托盘，进程继续驻留）。
2. **托盘偏好持久化**：接受启动瞬间托盘图标闪现后消失；本次不迁移到 Rust 配置。
3. **剪贴板失败**：静默 + stderr，不新增 UI toast。
4. **rayon 线程数**：默认 `min(4, cores-1)`，保留 `MIAOSOU_INDEX_THREADS` 覆盖。
