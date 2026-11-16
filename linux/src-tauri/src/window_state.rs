//! 窗口状态持久化：替代 v2 的 `tauri-plugin-window-state`
//!
//! v1 没有官方 window-state 插件，这里按插件 v2 的**文件格式**自实现一份，
//! 以便与 macOS 版配置互通（同一份 `.window-state.json` 结构）：
//!
//! ```json
//! { "main": { "width": 2400, "height": 1600, "x": 0, "y": 66,
//!             "prev_x": 4, "prev_y": 66, "maximized": true,
//!             "visible": true, "decorated": true, "fullscreen": false } }
//! ```
//!
//! 生命周期：
//!   1. `setup` 中 `restore_main_window()` 恢复几何并显示窗口（config 里 `visible=false` 防闪烁）；
//!   2. 焦点/移动/缩放在 `WindowEvent` 中触发，经 `WindowStateSaver` 去抖 800ms 落盘；
//!   3. `RunEvent::ExitRequested` / `Exit` 时 `save_now()` 同步兜底保存。
//!
//! 注意：Linux Wayland 下 `outer_position()` 可能失败、`set_position()` 被忽略，
//! 相关错误一律忽略；X11（UOS V20 默认 DDE 会话）下行为与 macOS 一致。

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::mpsc::{self, Sender},
    thread,
    time::Duration,
};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, Window};

const FILE_NAME: &str = ".window-state.json";
const MAIN_LABEL: &str = "main";
const SAVE_DEBOUNCE: Duration = Duration::from_millis(800);

fn default_true() -> bool {
    true
}

/// 单个窗口的状态。字段与 tauri-plugin-window-state v2 的落盘格式保持一致。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowStateEntry {
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    /// 最大化前的 x（插件会写，这里同样保留以兼容既有文件）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_x: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_y: Option<i32>,
    #[serde(default)]
    pub maximized: bool,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default = "default_true")]
    pub decorated: bool,
    #[serde(default)]
    pub fullscreen: bool,
}

impl Default for WindowStateEntry {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            x: 0,
            y: 0,
            prev_x: None,
            prev_y: None,
            maximized: false,
            visible: true,
            decorated: true,
            fullscreen: false,
        }
    }
}

/// `{ "main": {...} }`：JSON 顶层直接是「窗口 label → 状态」映射，与插件格式一致。
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WindowStateFile(pub BTreeMap<String, WindowStateEntry>);

/// 状态文件固定放在 `$XDG_CONFIG_HOME/miaosou/`（见 `paths.rs`，与 identifier 解耦）。
fn state_path() -> Option<PathBuf> {
    crate::paths::app_config_dir().map(|dir| dir.join(FILE_NAME))
}

pub fn read_state() -> WindowStateFile {
    state_path()
        .map(|path| read_state_from(&path))
        .unwrap_or_default()
}

fn write_state(file: &WindowStateFile) {
    if let Some(path) = state_path() {
        let _ = write_state_to(&path, file);
    }
}

// --- 纯函数（可单测） ---

fn read_state_from(path: &std::path::Path) -> WindowStateFile {
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn write_state_to(path: &std::path::Path, file: &WindowStateFile) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(file)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    // 先写临时文件再 rename，避免掉电/崩溃写出半个 JSON
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, path)
}

fn capture_window(window: &Window) -> Option<WindowStateEntry> {
    let size = window.inner_size().ok()?;
    let position = window.outer_position().ok();
    let maximized = window.is_maximized().unwrap_or(false);
    let fullscreen = window.is_fullscreen().unwrap_or(false);
    let visible = window.is_visible().unwrap_or(true);
    let decorated = window.is_decorated().unwrap_or(true);

    Some(WindowStateEntry {
        width: size.width,
        height: size.height,
        x: position.map(|p| p.x).unwrap_or(0),
        y: position.map(|p| p.y).unwrap_or(0),
        // 最大化时记录当前坐标，还原时作为非最大化位置（简化版 prev_x/prev_y）
        prev_x: if maximized {
            position.map(|p| p.x)
        } else {
            None
        },
        prev_y: if maximized {
            position.map(|p| p.y)
        } else {
            None
        },
        maximized,
        visible,
        decorated,
        fullscreen,
    })
}

/// 同步保存主窗口几何（退出、隐藏等关键路径使用）。
pub fn save_now(app: &AppHandle) {
    let Some(window) = app.get_window(MAIN_LABEL) else {
        return;
    };
    let Some(entry) = capture_window(&window) else {
        return;
    };
    let mut file = read_state();
    file.0.insert(MAIN_LABEL.to_string(), entry.clone());
    write_state(&file);
    eprintln!(
        "[miaosou] 已保存窗口几何: {}x{} @ ({},{}) maximized={} fullscreen={}",
        entry.width, entry.height, entry.x, entry.y, entry.maximized, entry.fullscreen
    );
}

/// 启动时恢复主窗口几何并显示窗口。
pub fn restore_main_window(app: &AppHandle) {
    let Some(window) = app.get_window(MAIN_LABEL) else {
        return;
    };

    if let Some(entry) = read_state().0.remove(MAIN_LABEL) {
        eprintln!(
            "[miaosou] 恢复窗口几何: {}x{} @ ({},{}) maximized={} fullscreen={}",
            entry.width, entry.height, entry.x, entry.y, entry.maximized, entry.fullscreen
        );
        if entry.width > 0 && entry.height > 0 {
            let _ = window.set_size(PhysicalSize::new(entry.width, entry.height));
        }
        let has_position = entry.x != 0 || entry.y != 0;
        if has_position && !entry.maximized {
            let _ = window.set_position(PhysicalPosition::new(entry.x, entry.y));
        }
        if entry.fullscreen {
            let _ = window.set_fullscreen(true);
        }
        if entry.maximized {
            let _ = window.maximize();
        }
    }

    // config 里窗口是 visible=false，恢复完几何再显示，避免闪烁
    let _ = window.show();
    let _ = window.set_focus();
}

/// 去抖保存器：窗口焦点/移动/缩放会高频触发，合并到最多每 800ms 一次落盘。
pub struct WindowStateSaver {
    tx: Sender<()>,
}

impl WindowStateSaver {
    pub fn start(app: &AppHandle) -> Self {
        let (tx, rx) = mpsc::channel::<()>();
        let app_handle = app.clone();
        let _ = thread::Builder::new()
            .name("window-state-saver".to_string())
            .spawn(move || {
                while rx.recv().is_ok() {
                    thread::sleep(SAVE_DEBOUNCE);
                    while rx.try_recv().is_ok() {}
                    save_now(&app_handle);
                }
            });
        Self { tx }
    }

    pub fn request(&self) {
        let _ = self.tx.send(());
    }
}

// ---------------------------------------------------------------------------
// 单元测试：文件格式必须与 tauri-plugin-window-state v2 完全兼容
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 取自 macOS 版 Cardinal（上游）实际落盘的 `.window-state.json`（插件 v2 格式）
    const REAL_MACOS_FILE: &str = r#"{
  "main": {
    "width": 2400,
    "height": 1600,
    "x": 0,
    "y": 66,
    "prev_x": 4,
    "prev_y": 66,
    "maximized": true,
    "visible": true,
    "decorated": true,
    "fullscreen": false
  }
}"#;

    #[test]
    fn parses_window_state_written_by_v2_plugin() {
        let file: WindowStateFile =
            serde_json::from_str(REAL_MACOS_FILE).expect("应能解析插件格式");
        let entry = file.0.get("main").expect("应包含 main 窗口");
        assert_eq!(entry.width, 2400);
        assert_eq!(entry.height, 1600);
        assert_eq!(entry.x, 0);
        assert_eq!(entry.y, 66);
        assert_eq!(entry.prev_x, Some(4));
        assert_eq!(entry.prev_y, Some(66));
        assert!(entry.maximized);
        assert!(entry.visible);
        assert!(entry.decorated);
        assert!(!entry.fullscreen);
    }

    #[test]
    fn round_trip_is_lossless() {
        let dir = std::env::temp_dir().join(format!("miaosou-window-state-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);

        let mut file = WindowStateFile::default();
        file.0.insert(
            "main".to_string(),
            WindowStateEntry {
                width: 1280,
                height: 720,
                x: 100,
                y: 50,
                ..WindowStateEntry::default()
            },
        );
        write_state_to(&path, &file).expect("写入应成功");

        let loaded = read_state_from(&path);
        let entry = loaded.0.get("main").unwrap();
        assert_eq!(
            (entry.width, entry.height, entry.x, entry.y),
            (1280, 720, 100, 50)
        );
        assert!(!entry.maximized);

        // 临时文件必须已被 rename 掉，避免留下 .json.tmp
        assert!(!path.with_extension("json.tmp").exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_or_corrupt_file_falls_back_to_default() {
        let path = std::env::temp_dir().join("miaosou-window-state-does-not-exist.json");
        let _ = std::fs::remove_file(&path);
        assert!(read_state_from(&path).0.is_empty());

        let corrupt = std::env::temp_dir().join("miaosou-window-state-corrupt.json");
        std::fs::write(&corrupt, "{ not json").unwrap();
        assert!(read_state_from(&corrupt).0.is_empty());
        std::fs::remove_file(&corrupt).ok();
    }

    #[test]
    fn unknown_window_labels_are_preserved() {
        let json = r#"{"main":{"width":800,"height":600},"settings":{"width":400,"height":300}}"#;
        let file: WindowStateFile = serde_json::from_str(json).unwrap();
        assert_eq!(file.0.len(), 2);
        assert_eq!(file.0["settings"].width, 400);
    }
}
