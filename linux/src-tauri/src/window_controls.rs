//! 窗口控制：v1 版 `window_controls.rs`
//!
//! 与上游 `cardinal/src-tauri/src/window_controls.rs` 行为一一对应，差异只在 API：
//!   - v2 `WebviewWindow` → v1 `Window`
//!   - v2 `window.emit(...)` 通过 `Emitter` trait → v1 `Window::emit` 固有方法
//!   - 每次操作后顺带通知窗口状态保存器（v1 无 window-state 插件，见 `window_state.rs`）
//!
//! Linux 额外注意：
//!   - Wayland 下 `set_position` 不可用（调用会被忽略），不影响功能；
//!   - DDE 下 `unminimize` 依赖窗口管理器支持，失败只记日志。

use crate::window_state::{self, WindowStateSaver};
use tauri::{AppHandle, Manager, Window};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowToggle {
    Hidden,
    Shown,
    Failed,
}

pub fn activate_window(window: &Window) {
    // 注意：v1 壳是 edition 2021，不能用 2024 的 let-chain，写成嵌套 if。
    if let Ok(true) = window.is_minimized() {
        if let Err(err) = window.unminimize() {
            eprintln!("[miaosou] 取消最小化失败: {err}");
        }
    }

    if let Ok(false) = window.is_visible() {
        if let Err(err) = window.show() {
            eprintln!("[miaosou] 显示窗口失败: {err}");
        }
    }

    if let Err(err) = window.set_focus() {
        eprintln!("[miaosou] 聚焦窗口失败: {err}");
    }
}

pub fn hide_window(window: &Window) -> bool {
    if let Err(err) = window.hide() {
        eprintln!("[miaosou] 隐藏窗口失败: {err}");
        return false;
    }
    true
}

pub fn trigger_quick_launch(window: &Window) {
    activate_window(window);

    if let Err(err) = window.emit("quick_launch", ()) {
        eprintln!("[miaosou] 发送 quick_launch 事件失败: {err}");
    }
}

/// 三态切换：前台且可见 → 隐藏；否则唤起（并发出 quick_launch，让前端重置搜索框）。
pub fn toggle_window(window: &Window) -> WindowToggle {
    let is_visible = window.is_visible().unwrap_or(true);
    let is_minimized = window.is_minimized().unwrap_or(false);
    let is_focused = window.is_focused().unwrap_or(false);

    if is_visible && !is_minimized && is_focused {
        if hide_window(window) {
            WindowToggle::Hidden
        } else {
            WindowToggle::Failed
        }
    } else {
        trigger_quick_launch(window);
        WindowToggle::Shown
    }
}

/// 与上游对齐：P2 的后台空闲刷盘/托盘逻辑会用到。
#[allow(dead_code)]
pub fn is_main_window_foreground(app_handle: &AppHandle) -> bool {
    let Some(window) = app_handle.get_window("main") else {
        return false;
    };

    let visible = window.is_visible().unwrap_or(false);
    let focused = window.is_focused().unwrap_or(false);
    let minimized = window.is_minimized().unwrap_or(false);

    visible && focused && !minimized
}

fn request_window_state_save(app: &AppHandle) {
    if let Some(saver) = app.try_state::<WindowStateSaver>() {
        saver.request();
    }
}

pub fn activate_main_window_impl(app: &AppHandle) {
    if let Some(window) = app.get_window("main") {
        activate_window(&window);
        request_window_state_save(app);
    } else {
        eprintln!("[miaosou] 请求激活主窗口，但窗口不存在");
    }
}

pub fn hide_main_window_impl(app: &AppHandle) {
    if let Some(window) = app.get_window("main") {
        hide_window(&window);
        request_window_state_save(app);
    } else {
        eprintln!("[miaosou] 请求隐藏主窗口，但窗口不存在");
    }
}

pub fn toggle_main_window_impl(app: &AppHandle) {
    if let Some(window) = app.get_window("main") {
        match toggle_window(&window) {
            WindowToggle::Hidden => {}
            WindowToggle::Shown | WindowToggle::Failed => {}
        }
        request_window_state_save(app);
    } else {
        eprintln!("[miaosou] 请求切换主窗口，但窗口不存在");
    }
}

/// 供窗口事件回调使用：焦点/移动/缩放后延迟保存几何。
pub fn note_window_geometry_changed(window: &Window) {
    // v1 的 Window::app_handle() 返回 owned AppHandle（v2 返回引用）
    let app = window.app_handle();
    if let Some(saver) = app.try_state::<WindowStateSaver>() {
        saver.request();
    } else {
        // 保存器可能尚未在 setup 中注册（极早期事件），直接同步保存兜底
        window_state::save_now(&app);
    }
}
