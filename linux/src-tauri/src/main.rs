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

mod commands;
mod icons;
mod identity;
mod index;
mod lifecycle;
mod native_ui;
mod paths;
mod reveal;
mod search_activity;
mod sort;
mod watcher;
mod window_controls;
mod window_state;

use index::{IndexConfig, IndexHandle};
use std::sync::{atomic::Ordering, Arc};
use tauri::{Manager, RunEvent, WindowEvent};

fn main() {
    let app = tauri::Builder::default()
        // 原生菜单 / 系统托盘：上游在前端用 v2 JS API 构建，v1 只能放 Rust 侧
        .menu(native_ui::build_app_menu())
        .on_menu_event(native_ui::handle_menu_event)
        .system_tray(native_ui::build_tray())
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
                    // 与上游一致：关闭窗口 = 隐藏到后台，进程继续驻留
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
