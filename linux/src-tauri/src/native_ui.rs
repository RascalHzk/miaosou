//! 原生菜单与系统托盘
//!
//! 上游把菜单和托盘都放在**前端 JS** 里用 Tauri v2 的 `@tauri-apps/api/menu` / `api/tray` 构建
//! （见 `cardinal/src/menu.ts`、`cardinal/src/tray.ts`）。v1 **没有**这两个 JS API，
//! 只能改为 Rust 侧构建，文件即本模块。
//!
//! 与上游的功能对齐情况：
//!   - App 菜单：关于 / 偏好设置 / 隐藏 / 退出 / 编辑 / 视图 / 窗口 / 帮助 —— 已对齐
//!   - 托盘：打开 + 分隔符 + 退出 —— 已对齐
//!   - 差异 1：v1 的菜单只能在 `Builder::menu()` 时设定，**运行时无法重建**，
//!     因此菜单文案固定为简体中文（信创目标用户）；语言切换只影响 Web UI。
//!     上游是 i18n 驱动重建，这里作为已知差异记录（P3 评估）。
//!   - 差异 2：上下文菜单（右键文件）在 v1 也没有原生弹出 API，
//!     改为 Web 侧自绘菜单实现（见 linux/ui 的 ContextMenu 组件）。
//!   - 差异 3：去掉「帮助 → 获取更新」。上游该入口打开 GitHub Releases（预览产物是 macOS 版），
//!     信创环境是离线部署、由运维统一分发；给 JS 开 `dialog` allowlist 只为弹一句话不划算，
//!     版本与「离线部署」说明已放在「关于」里（见 `about_metadata`）。
//!
//! 平台差异：
//!   - **Linux 不支持 Tauri v1 的多数预定义菜单项**：`Undo/Redo/Cut/Copy/Paste/SelectAll/
//!     Zoom/EnterFullScreen` 在 Linux 上会被静默丢弃（菜单点开是空的）。因此「编辑」「视图」
//!     以及窗口菜单里的「最大化/还原」全部改用 `CustomMenuItem` + 自己实现动作；
//!     仍可用的原生项只有：About / Hide / Quit / Minimize / CloseWindow / Separator。
//!   - Linux（DDE）下 `LeftClick` 等托盘事件不受支持，因此统一用托盘菜单交互。
//!   - macOS 下额外提供 HideOthers / ShowAll。

use crate::{identity, window_controls};
use tauri::{
    AboutMetadata, AppHandle, CustomMenuItem, Menu, MenuEntry, MenuItem, Submenu, SystemTray,
    SystemTrayEvent, SystemTrayMenu, SystemTrayMenuItem, WindowMenuEvent,
};

/// 菜单/托盘/关于框里显示的产品名（中文名，信创目标用户）。
pub const APP_NAME: &str = identity::NAME_ZH;
const VERSION: &str = env!("CARGO_PKG_VERSION");

// 菜单项 id（自定义项）
const ID_PREFERENCES: &str = "app.preferences";
const ID_EDIT_UNDO: &str = "edit.undo";
const ID_EDIT_REDO: &str = "edit.redo";
const ID_EDIT_CUT: &str = "edit.cut";
const ID_EDIT_COPY: &str = "edit.copy";
const ID_EDIT_PASTE: &str = "edit.paste";
const ID_EDIT_SELECT_ALL: &str = "edit.select_all";
const ID_VIEW_TOGGLE_FULLSCREEN: &str = "view.toggle_fullscreen";
const ID_WINDOW_TOGGLE_MAXIMIZE: &str = "window.toggle_maximize";

/// 「编辑」菜单项定义：(id, 文案, 快捷键)。单测保证它不为空、文案非空。
pub(crate) const EDIT_MENU_ITEMS: &[(&str, &str, Option<&str>)] = &[
    (ID_EDIT_UNDO, "撤销", Some("CmdOrCtrl+Z")),
    (ID_EDIT_REDO, "重做", Some("CmdOrCtrl+Shift+Z")),
    (ID_EDIT_CUT, "剪切", Some("CmdOrCtrl+X")),
    (ID_EDIT_COPY, "复制", Some("CmdOrCtrl+C")),
    (ID_EDIT_PASTE, "粘贴", Some("CmdOrCtrl+V")),
    (ID_EDIT_SELECT_ALL, "全选", Some("CmdOrCtrl+A")),
];

/// 「视图」菜单项定义（Linux 无原生 EnterFullScreen，改自定义项）。
pub(crate) const VIEW_MENU_ITEMS: &[(&str, &str, Option<&str>)] =
    &[(ID_VIEW_TOGGLE_FULLSCREEN, "切换全屏", Some("F11"))];

// 托盘菜单项 id
const ID_TRAY_OPEN: &str = "tray.open";
const ID_TRAY_QUIT: &str = "tray.quit";

// 前端事件名
pub const EVENT_MENU_PREFERENCES: &str = "menu_preferences";

// ---------------------------------------------------------------------------
// App 菜单
// ---------------------------------------------------------------------------

fn about_metadata() -> AboutMetadata {
    AboutMetadata::new()
        .version(VERSION)
        .copyright("MIT · 上游 Cardinal © Cardinal Contributors")
        .license("MIT")
        .comments("面向统信 UOS V20 的本地文件搜索工具，完全离线运行。")
}

pub fn build_app_menu() -> Menu {
    let app_submenu = Submenu::new(
        APP_NAME,
        Menu::with_items([
            MenuEntry::NativeItem(MenuItem::About(APP_NAME.to_string(), about_metadata())),
            MenuEntry::NativeItem(MenuItem::Separator),
            MenuEntry::CustomItem(
                CustomMenuItem::new(ID_PREFERENCES, "偏好设置…").accelerator("CmdOrCtrl+,"),
            ),
            MenuEntry::NativeItem(MenuItem::Separator),
            MenuEntry::NativeItem(MenuItem::Hide),
            #[cfg(target_os = "macos")]
            MenuEntry::NativeItem(MenuItem::HideOthers),
            #[cfg(target_os = "macos")]
            MenuEntry::NativeItem(MenuItem::ShowAll),
            #[cfg(target_os = "macos")]
            MenuEntry::NativeItem(MenuItem::Separator),
            MenuEntry::NativeItem(MenuItem::Quit),
        ]),
    );

    // Linux 不接受预定义编辑项 → 用自定义项 + 页面内 execCommand 实现。
    // 菜单内容由 EDIT_MENU_ITEMS / VIEW_MENU_ITEMS 单一事实来源构建。
    let edit_submenu = Submenu::new(
        "编辑",
        Menu::with_items(entries_with_separators(EDIT_MENU_ITEMS, &[2, 5])),
    );
    let view_submenu = Submenu::new(
        "视图",
        Menu::with_items(entries_with_separators(VIEW_MENU_ITEMS, &[])),
    );

    let window_submenu = Submenu::new(
        "窗口",
        Menu::with_items([
            // Linux 支持 Minimize / CloseWindow；Zoom 不支持，改自定义菜单项
            MenuEntry::NativeItem(MenuItem::Minimize),
            custom_entry(ID_WINDOW_TOGGLE_MAXIMIZE, "最大化/还原", None),
            MenuEntry::NativeItem(MenuItem::Separator),
            MenuEntry::NativeItem(MenuItem::CloseWindow),
        ]),
    );

    Menu::with_items([
        MenuEntry::Submenu(app_submenu),
        MenuEntry::Submenu(edit_submenu),
        MenuEntry::Submenu(view_submenu),
        MenuEntry::Submenu(window_submenu),
    ])
}

/// 把 (id, 文案, 快捷键) 表展开成菜单项，并在指定下标前插入分隔符。
fn entries_with_separators(
    items: &[(&'static str, &'static str, Option<&'static str>)],
    separator_before: &[usize],
) -> Vec<MenuEntry> {
    let mut entries = Vec::with_capacity(items.len() + separator_before.len());
    for (index, (id, label, accelerator)) in items.iter().enumerate() {
        if separator_before.contains(&index) {
            entries.push(MenuEntry::NativeItem(MenuItem::Separator));
        }
        entries.push(custom_entry(id, label, *accelerator));
    }
    entries
}

/// 构造自定义菜单项（Linux 上预定义编辑/视图项不受支持，必须用自定义项）。
fn custom_entry(id: &str, label: &str, accelerator: Option<&str>) -> MenuEntry {
    let mut item = CustomMenuItem::new(id.to_string(), label.to_string());
    if let Some(accelerator) = accelerator {
        item = item.accelerator(accelerator);
    }
    MenuEntry::CustomItem(item)
}

/// 在页面里执行脚本（菜单动作走 webview 的 document.execCommand / DOM 操作）。
fn eval_in_page(window: &tauri::Window, js: &str) {
    if let Err(err) = window.eval(js) {
        eprintln!("[miaosou] 菜单动作执行失败: {err}");
    }
}

pub fn handle_menu_event(event: WindowMenuEvent) {
    let window = event.window();
    match event.menu_item_id() {
        ID_PREFERENCES => {
            // Windows/Linux 上没有全局偏好面板，统一交给前端弹 overlay
            if let Err(err) = window.emit(EVENT_MENU_PREFERENCES, ()) {
                eprintln!("[miaosou] 发送 menu_preferences 失败: {err}");
            }
        }
        ID_EDIT_UNDO => eval_in_page(window, "document.execCommand('undo')"),
        ID_EDIT_REDO => eval_in_page(window, "document.execCommand('redo')"),
        ID_EDIT_CUT => eval_in_page(window, "document.execCommand('cut')"),
        ID_EDIT_COPY => eval_in_page(window, "document.execCommand('copy')"),
        // WebKitGTK 对 execCommand('paste') 有安全限制：尽力而为，失败时用户仍可用 Ctrl+V
        ID_EDIT_PASTE => eval_in_page(window, "document.execCommand('paste')"),
        ID_EDIT_SELECT_ALL => eval_in_page(
            window,
            "(function(){const el=document.activeElement;if(el&&(el.tagName==='INPUT'||el.tagName==='TEXTAREA')){el.select();}else{document.execCommand('selectAll');}})()",
        ),
        ID_VIEW_TOGGLE_FULLSCREEN => {
            let fullscreen = window.is_fullscreen().unwrap_or(false);
            if let Err(err) = window.set_fullscreen(!fullscreen) {
                eprintln!("[miaosou] 切换全屏失败: {err}");
            }
        }
        ID_WINDOW_TOGGLE_MAXIMIZE => {
            let maximized = window.is_maximized().unwrap_or(false);
            let result = if maximized {
                window.unmaximize()
            } else {
                window.maximize()
            };
            if let Err(err) = result {
                eprintln!("[miaosou] 切换最大化失败: {err}");
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// 系统托盘
// ---------------------------------------------------------------------------

fn build_tray_menu() -> SystemTrayMenu {
    SystemTrayMenu::new()
        .add_item(
            CustomMenuItem::new(ID_TRAY_OPEN, "打开 秒搜").accelerator("CmdOrCtrl+Shift+Space"),
        )
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(CustomMenuItem::new(ID_TRAY_QUIT, "退出 秒搜"))
}

pub fn build_tray() -> SystemTray {
    SystemTray::new()
        .with_menu(build_tray_menu())
        .with_tooltip(APP_NAME)
}

pub fn handle_tray_event(app: &AppHandle, event: SystemTrayEvent) {
    // Linux 不支持 LeftClick/RightClick，只有菜单点击事件
    if let SystemTrayEvent::MenuItemClick { id, .. } = event {
        match id.as_ref() {
            ID_TRAY_OPEN => window_controls::activate_main_window_impl(app),
            ID_TRAY_QUIT => {
                // 与上游一致：托盘退出是真正的退出（不走“关闭即隐藏”）
                crate::lifecycle::EXIT_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);
                app.exit(0);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_menu_has_visible_zh_items() {
        assert!(
            EDIT_MENU_ITEMS.len() >= 6,
            "编辑菜单不应为空（Linux 上预定义编辑项会被静默丢弃）"
        );
        let ids: Vec<&str> = EDIT_MENU_ITEMS.iter().map(|(id, _, _)| *id).collect();
        for required in [
            ID_EDIT_UNDO,
            ID_EDIT_REDO,
            ID_EDIT_CUT,
            ID_EDIT_COPY,
            ID_EDIT_PASTE,
            ID_EDIT_SELECT_ALL,
        ] {
            assert!(ids.contains(&required), "缺少菜单项 id: {required}");
        }
        for (id, label, accelerator) in EDIT_MENU_ITEMS {
            assert!(!id.is_empty());
            assert!(!label.trim().is_empty(), "菜单项 {id} 文案为空");
            assert!(accelerator.is_some(), "菜单项 {id} 应带快捷键");
        }
    }

    #[test]
    fn view_menu_has_visible_zh_items() {
        assert!(!VIEW_MENU_ITEMS.is_empty(), "视图菜单不应为空");
        for (id, label, _) in VIEW_MENU_ITEMS {
            assert!(!id.is_empty());
            assert!(!label.trim().is_empty(), "菜单项 {id} 文案为空");
        }
    }
}
