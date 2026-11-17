//! 秒搜（miaosou）—— Tauri v1 壳入口
//!
//! 目标环境：统信 UOS V20 / x86_64（Debian 10 基座，webkit2gtk 4.0 / libsoup2）
//!
//! 本文件只做「装配」，模块划分对齐上游 `cardinal/src-tauri/src/`：
//!   - `commands.rs`        IPC 命令（搜索 / 索引 / 窗口 / 系统集成）
//!   - `index.rs`           真实索引管线（fswalk + search-cache + slab-mmap 持久化）
//!   - `icons.rs`           freedesktop 图标主题 + 缩略图缓存（替代上游 fs-icon）
//!   - `reveal.rs`          DDE/GNOME/XFCE/KDE 定位与默认程序打开（替代 macOS `open -R`）
//!   - `sort.rs`            远程排序（移植自上游）
//!   - `window_controls.rs` 窗口显示、隐藏、三态切换、前台判定
//!   - `window_state.rs`    窗口几何持久化（替代 v2 的 window-state 插件）
//!   - `native_ui.rs`       原生菜单 + 系统托盘（替代 v2 前端 JS 构建）
//!   - `lifecycle.rs`       应用生命周期状态（app_lifecycle_state 事件）
//!   - `watcher.rs`         文件系统实时监控（inotify，P2.5）
//!   - `search_activity.rs` 搜索活跃度（空闲刷盘依据）
//!
//! 启动时序（与上游一致）：壳先起来 → 前端挂载后调用 `start_logic` 下发监控配置
//! → `index.rs` 优先加载持久化索引，失败则后台全量扫描。

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

#[cfg(target_os = "linux")]
mod clipboard;
mod commands;
mod icons;
mod identity;
mod index;
mod lifecycle;
mod native_ui;
mod paths;
mod reveal;
mod search_activity;
mod single_instance;
mod sort;
mod watcher;
mod window_controls;
mod window_state;

use index::{IndexConfig, IndexHandle};
use std::sync::{atomic::Ordering, Arc};
use tauri::{Manager, RunEvent, WindowEvent};

fn main() {
    // 单实例：必须在 Tauri 初始化之前，第二个进程只负责唤起首实例。
    if !single_instance::acquire() {
        return;
    }
    configure_index_threads();

    let app = tauri::Builder::default()
        // 原生菜单 / 系统托盘：上游在前端用 v2 JS API 构建，v1 只能放 Rust 侧
        .menu(native_ui::build_app_menu())
        .on_menu_event(native_ui::handle_menu_event)
        .on_system_tray_event(native_ui::handle_tray_event)
        .invoke_handler(tauri::generate_handler![
            commands::search,
            commands::get_nodes_info,
            commands::get_sorted_view,
            commands::update_icon_viewport,
            commands::get_app_status,
            commands::get_index_info,
            commands::start_logic,
            commands::set_watch_config,
            commands::trigger_rescan,
            commands::open_in_finder,
            commands::open_path,
            commands::get_default_watch_root,
            commands::copy_files_to_clipboard,
            commands::toggle_quicklook,
            commands::close_quicklook,
            commands::update_quicklook,
            commands::hide_main_window,
            commands::activate_main_window,
            commands::toggle_main_window,
            commands::set_tray_activation_policy,
            commands::set_global_shortcut,
            commands::frontend_ready,
        ])
        .setup(|app| {
            let handle = app.handle();

            // 首实例已就绪：后续第二次启动的 SHOW 请求可以唤起主窗口。
            single_instance::attach(handle.clone());

            // 托盘句柄：偏好设置里的“显示托盘图标”用它做运行时销毁/重建。
            // 放在 setup 里创建，避免 builder 创建失败时 app.tray_handle() panic。
            let tray = match native_ui::build_tray().build(&handle) {
                Ok(handle) => Some(handle),
                Err(err) => {
                    eprintln!("[miaosou] 创建系统托盘失败: {err}");
                    None
                }
            };
            app.manage(native_ui::TrayState::from_option(tray));

            // 启动横幅：现场排查时一眼看到「哪个产品、哪个版本、哪个 identifier」。
            eprintln!(
                "{} {} v{}（id={}）启动",
                identity::LOG_PREFIX,
                identity::NAME_EN,
                env!("CARGO_PKG_VERSION"),
                identity::APP_ID
            );

            // 1) 旧版本（上游 Cardinal 时期）的数据目录一次性迁移，再定位索引文件
            paths::migrate_legacy_config_dir();
            let default_config = IndexConfig::new(
                index::default_watch_root(),
                index::default_ignore_paths(),
                Vec::new(),
            );
            app.manage(IndexHandle::new(default_config, paths::index_db_path()));

            // 2) 恢复窗口几何并显示窗口（config 中 visible=false，避免启动闪烁）
            window_state::restore_main_window(&handle);
            app.manage(window_state::WindowStateSaver::start(&handle));

            // 3) 全局快捷键兜底注册（前端也会调用 set_global_shortcut，命令内部做了幂等）
            use tauri::GlobalShortcutManager;
            let handle_for_shortcut = handle.clone();
            let mut manager = handle.global_shortcut_manager();
            if let Err(err) = manager.register("CmdOrCtrl+Shift+Space", move || {
                window_controls::toggle_main_window_impl(&handle_for_shortcut);
            }) {
                eprintln!("[miaosou] 注册全局快捷键失败（Rootless/Wayland 环境可忽略）: {err}");
            }

            // 4) 生命周期：等待前端调用 start_logic 后才会进入 Ready
            lifecycle::emit_app_state(&handle);
            Ok(())
        })
        .on_window_event(|event| {
            // v1 传入单个 GlobalWindowEvent：window() + event()
            let window = event.window();
            if window.label() != "main" {
                return;
            }

            match event.event() {
                WindowEvent::CloseRequested { api, .. } => {
                    if lifecycle::EXIT_REQUESTED.load(Ordering::Relaxed) {
                        return;
                    }

                    let tray_enabled = window
                        .app_handle()
                        .try_state::<native_ui::TrayState>()
                        .map(|state| state.is_enabled())
                        .unwrap_or(true);
                    if !tray_enabled {
                        // 偏好里关闭了托盘：没有“隐藏到托盘”的目标，按普通应用退出。
                        // 不 prevent_close，交给 ExitRequested 做落盘。
                        return;
                    }

                    // 托盘开启：与上游一致，关闭窗口 = 隐藏到后台，进程继续驻留。
                    api.prevent_close();
                    window_controls::hide_window(window);
                    window_state::save_now(&window.app_handle());
                }
                WindowEvent::Focused(_) | WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                    window_controls::note_window_geometry_changed(window);
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building miaosou");

    app.run(|handle, event| match event {
        // v1 的 ExitRequested 只有 `api` 字段；用 EXIT_REQUESTED 标志区分首次/二次退出。
        RunEvent::ExitRequested { api, .. } => {
            let already_requested = lifecycle::EXIT_REQUESTED.swap(true, Ordering::Relaxed);
            window_state::save_now(handle);
            flush_index(handle);
            if !already_requested {
                // 先落盘再真正退出（避免丢窗口状态与索引）
                api.prevent_exit();
                handle.exit(0);
            }
        }
        RunEvent::Exit => {
            lifecycle::APP_QUIT.store(true, Ordering::Relaxed);
            window_state::save_now(handle);
            flush_index(handle);
        }
        _ => {}
    });
}

/// 退出时把内存索引落盘（消费掉缓存；重复调用是 no-op）。
fn flush_index(handle: &tauri::AppHandle) {
    let Some(state) = handle.try_state::<Arc<IndexHandle>>() else {
        return;
    };
    if let Err(err) = state.inner().flush() {
        eprintln!("[miaosou] 退出时保存索引失败: {err}");
    }
}

/// 限制索引/扫描的 rayon 线程数，避免首次全量扫描把 UI/WebKit 饿死。
///
/// 默认 `min(4, cores-1)`；可用 `MIAOSOU_INDEX_THREADS` 覆盖。
fn configure_index_threads() {
    let default_threads = std::thread::available_parallelism()
        .map(|count| count.get().saturating_sub(1).clamp(1, 4))
        .unwrap_or(2);
    let threads = std::env::var("MIAOSOU_INDEX_THREADS")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default_threads);

    if let Err(err) = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .thread_name(|index| format!("miaosou-index-{index}"))
        .build_global()
    {
        eprintln!("[miaosou] 初始化索引线程池失败（{err}），继续使用 rayon 默认值");
    } else {
        eprintln!("[miaosou] 索引/扫描线程数：{threads}");
    }
}
