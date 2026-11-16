//! 应用生命周期状态：v1 版 `lifecycle.rs`
//!
//! 与上游一一对应，只把 `Emitter::emit` 换成 v1 的 `Manager::emit_all`。
//! 前端订阅 `app_lifecycle_state` 事件显示「初始化中 / 更新中 / 就绪」。

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use tauri::{AppHandle, Manager};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppLifecycleState {
    Initializing = 0,
    Updating = 1,
    Ready = 2,
}

impl AppLifecycleState {
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Initializing,
            1 => Self::Updating,
            2 => Self::Ready,
            _ => Self::Initializing,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Initializing => "Initializing",
            Self::Updating => "Updating",
            Self::Ready => "Ready",
        }
    }
}

static APP_LIFECYCLE_STATE: AtomicU8 = AtomicU8::new(AppLifecycleState::Initializing as u8);

pub static APP_QUIT: AtomicBool = AtomicBool::new(false);
pub static EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn load_app_state() -> AppLifecycleState {
    AppLifecycleState::from_u8(APP_LIFECYCLE_STATE.load(Ordering::Acquire))
}

pub fn store_app_state(state: AppLifecycleState) {
    APP_LIFECYCLE_STATE.store(state as u8, Ordering::Release);
}

pub fn emit_app_state(app_handle: &AppHandle) {
    if let Err(err) = app_handle.emit_all("app_lifecycle_state", load_app_state().as_str()) {
        eprintln!("[miaosou] 发送 app_lifecycle_state 失败: {err}");
    }
}

pub fn update_app_state(app_handle: &AppHandle, state: AppLifecycleState) {
    if load_app_state() == state {
        return;
    }
    store_app_state(state);
    emit_app_state(app_handle);
}
