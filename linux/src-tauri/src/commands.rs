//! IPC 命令层（v1 版 `commands.rs`）
//!
//! P2 起接入真实索引管线（`crate::index`）：命令契约与上游 `cardinal/src-tauri/src/commands.rs`
//! 保持一致，前端无需改动即可拿到真实搜索结果。
//!
//! 与上游的实现差异（都是刻意的阶段性简化）：
//!   - 搜索直接对 `Mutex<SearchCache>` 加锁执行，没有上游那套「后台线程 + mpsc」请求协议；
//!   - 图标走 freedesktop 图标主题 + 缩略图缓存（`icons.rs`）；
//!   - Quick Look 仍是占位实现（Linux 没有 QLPreviewPanel，P3 改内置 WebView 预览）。

use crate::{
    identity,
    identity::LOG_PREFIX,
    index::{default_ignore_paths, home_dir, IndexConfig, IndexHandle},
    search_activity,
    sort::{sort_entries, SortEntry, SortStatePayload},
    window_controls::{activate_main_window_impl, hide_main_window_impl, toggle_main_window_impl},
};
use search_cache::{SlabIndex, SlabNodeMetadata, SlabNodeMetadataCompact};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};
use tauri::{AppHandle, State};

pub type SharedIndex = std::sync::Arc<IndexHandle>;

// ---------------------------------------------------------------------------
// 入参 / 出参类型（对齐上游）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOptionsPayload {
    #[serde(default)]
    pub case_insensitive: bool,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub results: Vec<SlabIndex>,
    pub highlights: Vec<String>,
    pub status_code: u8,
}

impl SearchResponse {
    pub const OK: u8 = 0;
    #[allow(dead_code)]
    pub const CANCELLED: u8 = 1;
}

#[derive(Serialize)]
pub struct NodeInfoMetadata {
    pub r#type: u8,
    pub size: i64,
    pub ctime: u32,
    pub mtime: u32,
}

impl NodeInfoMetadata {
    fn from_metadata(metadata: SlabNodeMetadata<'_>) -> Self {
        Self {
            r#type: metadata.r#type() as u8,
            size: metadata.size(),
            ctime: metadata
                .ctime()
                .map(|value| value.get())
                .unwrap_or_default(),
            mtime: metadata
                .mtime()
                .map(|value| value.get())
                .unwrap_or_default(),
        }
    }

    fn from_compact(metadata: &SlabNodeMetadataCompact) -> Option<Self> {
        metadata.as_ref().map(Self::from_metadata)
    }
}

#[derive(Serialize)]
pub struct NodeInfo {
    pub path: String,
    pub metadata: Option<NodeInfoMetadata>,
    pub icon: Option<String>,
}

// ---------------------------------------------------------------------------
// 搜索 / 节点信息 / 排序
// ---------------------------------------------------------------------------

/// 搜索。入参形状对齐上游：`{ directoryQuery, query, options }`。
#[tauri::command]
pub async fn search(
    directory_query: Option<String>,
    query: Option<String>,
    options: Option<SearchOptionsPayload>,
    state: State<'_, SharedIndex>,
) -> Result<SearchResponse, String> {
    search_activity::note_search_activity();

    let index = state.inner().clone();
    let case_insensitive = options.map(|opts| opts.case_insensitive).unwrap_or(false);

    tauri::async_runtime::spawn_blocking(move || -> Result<SearchResponse, String> {
        let started = Instant::now();
        let outcome = index.search(directory_query, query, case_insensitive)?;
        let (status_code, results) = match outcome.nodes {
            Some(nodes) => (SearchResponse::OK, nodes),
            None => (SearchResponse::CANCELLED, Vec::new()),
        };

        if identity::trace_enabled() {
            eprintln!(
                "[miaosou] search: {} 条命中，用时 {}ms",
                results.len(),
                started.elapsed().as_millis()
            );
        }

        Ok(SearchResponse {
            results,
            highlights: outcome.highlights,
            status_code,
        })
    })
    .await
    .map_err(|err| format!("搜索任务失败: {err}"))?
}

/// 按槽位取节点元数据（上游同名命令）。
#[tauri::command]
pub async fn get_nodes_info(
    results: Vec<SlabIndex>,
    include_icons: Option<bool>,
    state: State<'_, SharedIndex>,
) -> Result<Vec<NodeInfo>, String> {
    // 默认带图标：前端一次只加载一屏（约 40 行），懒加载 + 图标缓存足够快
    let include_icons = include_icons.unwrap_or(true);

    let index = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        index
            .expand_nodes(&results)
            .into_iter()
            .map(|node| {
                let icon = if include_icons {
                    crate::icons::row_icon_data_url(&node.path)
                } else {
                    None
                };
                NodeInfo {
                    path: node.path.to_string_lossy().into_owned(),
                    metadata: NodeInfoMetadata::from_compact(&node.metadata),
                    icon,
                }
            })
            .collect()
    })
    .await
    .map_err(|err| format!("读取节点信息失败: {err}"))
}

/// 远程排序（把排序下推到 Rust 侧，避免前端对大结果集排序）。
#[tauri::command]
pub async fn get_sorted_view(
    results: Vec<SlabIndex>,
    sort: Option<SortStatePayload>,
    state: State<'_, SharedIndex>,
) -> Result<Vec<SlabIndex>, String> {
    let Some(sort_state) = sort else {
        return Ok(results);
    };
    if results.is_empty() {
        return Ok(results);
    }

    let index = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let nodes = index.expand_nodes(&results);
        if nodes.len() != results.len() {
            // 索引在查询间隙被重建，保持原顺序即可
            return results;
        }

        let mut entries: Vec<SortEntry> = results
            .into_iter()
            .zip(nodes)
            .map(|(slab_index, node)| SortEntry::new(slab_index, node))
            .collect();
        sort_entries(&mut entries, &sort_state);
        entries.into_iter().map(|entry| entry.slab_index).collect()
    })
    .await
    .map_err(|err| format!("排序失败: {err}"))
}

#[tauri::command]
pub fn update_icon_viewport(id: u64, viewport: Vec<SlabIndex>, state: State<'_, SharedIndex>) {
    // 图标已经随 get_nodes_info 一起返回（带图标名缓存），这里只做「视口预热」：
    // 提前把用户即将看到的图标查出来，避免滚动时第一次出现空白。
    let index = state.inner().clone();
    // 预热任务即发即忘（JoinHandle 丢弃后继续执行）
    tauri::async_runtime::spawn_blocking(move || {
        for node in index.expand_nodes(&viewport) {
            let _ = crate::icons::row_icon_data_url(&node.path);
        }
    });
    let _ = id;
}

// ---------------------------------------------------------------------------
// 索引状态 / 配置 / 重建
// ---------------------------------------------------------------------------

/// 只返回生命周期状态字符串（与上游一致，前端用它判断“索引是否就绪”）。
#[tauri::command]
pub fn get_app_status() -> String {
    crate::lifecycle::load_app_state().as_str().to_string()
}

/// 诊断用：返回索引的详细状态（前端暂不消费）。
#[tauri::command]
pub async fn get_index_info(state: State<'_, SharedIndex>) -> Result<String, String> {
    let index = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || index.debug_summary())
        .await
        .map_err(|err| format!("读取索引状态失败: {err}"))
}

/// 前端启动时调用：下发监控配置并确保索引就绪。
#[tauri::command]
pub async fn start_logic(
    watch_root: String,
    ignore_paths: Vec<String>,
    include_paths: Option<Vec<String>>,
    app: AppHandle,
    state: State<'_, SharedIndex>,
) -> Result<(), String> {
    let index = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        apply_watch_config_blocking(watch_root, ignore_paths, include_paths, app, index)
    })
    .await
    .map_err(|err| format!("启动索引任务失败: {err}"))?
}

/// 偏好设置变更时调用：配置变化 → 重建；未变 → 直接复用持久化索引。
#[tauri::command]
pub async fn set_watch_config(
    watch_root: String,
    ignore_paths: Vec<String>,
    include_paths: Option<Vec<String>>,
    app: AppHandle,
    state: State<'_, SharedIndex>,
) -> Result<(), String> {
    let index = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        apply_watch_config_blocking(watch_root, ignore_paths, include_paths, app, index)
    })
    .await
    .map_err(|err| format!("更新监控配置任务失败: {err}"))?
}

#[tauri::command]
pub fn trigger_rescan(app: AppHandle, state: State<'_, SharedIndex>) -> Result<(), String> {
    state.inner().force_rebuild(&app);
    Ok(())
}

fn apply_watch_config_blocking(
    watch_root: String,
    ignore_paths: Vec<String>,
    include_paths: Option<Vec<String>>,
    app: AppHandle,
    index: Arc<IndexHandle>,
) -> Result<(), String> {
    // 验收/冒烟用：MIAOSOU_WATCH_ROOT 强制覆盖监控根。必须在构建配置**之前**生效：
    // 否则「忽略规则与监控根冲突」的清理会作用在旧根上。
    // （现场踩过：覆盖到 /tmp 下验收时，默认忽略表里的 /tmp 让整棵树索引为空。）
    let override_root = identity::watch_root_override()
        .and_then(|raw| normalize_path_input(&raw))
        .map(|path| {
            eprintln!("{LOG_PREFIX} MIAOSOU_WATCH_ROOT 覆盖监控根：{path:?}");
            path
        });
    let effective_root = override_root
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or(watch_root);

    let config = build_index_config(
        &effective_root,
        ignore_paths,
        include_paths.unwrap_or_default(),
    )?;
    index.ensure_ready(&app, config);
    Ok(())
}

// ---------------------------------------------------------------------------
// 路径规范化
// ---------------------------------------------------------------------------

fn normalize_path_input(raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut expanded = PathBuf::new();
    for (index, component) in Path::new(trimmed).components().enumerate() {
        if index == 0 && component.as_os_str() == "~" {
            expanded.push(home_dir()?);
        } else {
            expanded.push(component);
        }
    }

    expanded.is_absolute().then_some(expanded)
}

fn build_index_config(
    watch_root: &str,
    ignore_paths: Vec<String>,
    include_paths: Vec<String>,
) -> Result<IndexConfig, String> {
    let root = normalize_path_input(watch_root)
        .ok_or_else(|| format!("watch_root 必须是绝对路径或 ~ 开头：{watch_root:?}"))?;

    let mut ignores: Vec<PathBuf> = ignore_paths
        .into_iter()
        .filter_map(|path| {
            let normalized = normalize_path_input(&path);
            if normalized.is_none() {
                eprintln!("[miaosou] 忽略无效的 ignore 路径：{path:?}");
            }
            normalized
        })
        .collect();

    // 去重但保持顺序
    ignores.sort();
    ignores.dedup();

    if ignores.is_empty() {
        ignores = default_ignore_paths();
    }

    // 监控根若落在某条忽略路径之下（例如验收时把根设成 /tmp/xc 或某个缓存目录），
    // 那条忽略规则会让整棵树索引为空。显式配置的根优先：剔除「根的祖先/自身」级忽略项。
    let shadowing: Vec<PathBuf> = ignores
        .iter()
        .filter(|ignore| root == **ignore || root.starts_with(ignore.as_path()))
        .cloned()
        .collect();
    if !shadowing.is_empty() {
        for ignore in &shadowing {
            eprintln!("{LOG_PREFIX} 忽略路径 {ignore:?} 覆盖了监控根 {root:?}，已忽略该规则");
        }
        ignores.retain(|ignore| !shadowing.contains(ignore));
    }

    let includes: Vec<PathBuf> = include_paths
        .into_iter()
        .filter_map(|path| normalize_path_input(&path))
        .collect();

    Ok(IndexConfig::new(root, ignores, includes))
}

// ---------------------------------------------------------------------------
// 系统集成（打开 / 定位）
// ---------------------------------------------------------------------------

/// 平台默认监控根：前端设置页用它显示 Debian/UOS 的真实主目录（如 `/home/uos`），
/// 而不是字面量 `~`（Rust 侧仍会把 `~` 展开，这里只是让默认值可读、可核对）。
#[tauri::command]
pub fn get_default_watch_root() -> String {
    crate::index::default_watch_root()
        .to_string_lossy()
        .into_owned()
}

#[tauri::command]
pub async fn open_in_finder(path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || crate::reveal::reveal(Path::new(&path)))
        .await
        .map_err(|err| format!("定位任务失败: {err}"))?
}

#[tauri::command]
pub async fn open_path(path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || crate::reveal::open(Path::new(&path)))
        .await
        .map_err(|err| format!("打开任务失败: {err}"))?
}

/// 复制文件到剪贴板：用户可在 DDE/GNOME 文件管理器里直接 Ctrl+V。
///
/// Linux 走 GTK clipboard（见 `clipboard.rs`）：DDE 需要
/// `x-special/gnome-copied-files`，arboard 只提供 `text/uri-list`。
/// 其它平台继续用 arboard，设置后由系统负责剪贴板生命周期。
#[tauri::command]
pub fn copy_files_to_clipboard(app: AppHandle, paths: Vec<String>) {
    let candidates = normalize_clipboard_paths(paths);
    if candidates.is_empty() {
        eprintln!("{LOG_PREFIX} 复制文件：没有可用的绝对路径");
        return;
    }

    #[cfg(target_os = "linux")]
    {
        if let Err(err) = crate::clipboard::copy_files(&app, candidates) {
            eprintln!("{LOG_PREFIX} 复制文件到剪贴板失败: {err}");
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = &app;
        let spawned = std::thread::Builder::new()
            .name("miaosou-clipboard".to_string())
            .spawn(move || copy_paths_to_clipboard(candidates));
        if let Err(err) = spawned {
            eprintln!("{LOG_PREFIX} 启动剪贴板线程失败: {err}");
        }
    }
}

/// 只接受存在且是绝对路径的条目（`~` 由前端/Rust 展开后传入）。
fn normalize_clipboard_paths(paths: Vec<String>) -> Vec<PathBuf> {
    let mut normalized: Vec<PathBuf> = Vec::with_capacity(paths.len());
    for raw in paths {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let expanded = if let Some(rest) = trimmed.strip_prefix("~/") {
            match home_dir() {
                Some(home) => home.join(rest),
                None => continue,
            }
        } else {
            PathBuf::from(trimmed)
        };
        if !expanded.is_absolute() || !expanded.exists() {
            eprintln!("{LOG_PREFIX} 跳过无效的复制路径: {expanded:?}");
            continue;
        }
        normalized.push(expanded);
    }
    normalized
}

#[cfg(not(target_os = "linux"))]
fn copy_paths_to_clipboard(paths: Vec<PathBuf>) {
    let mut clipboard = match arboard::Clipboard::new() {
        Ok(clipboard) => clipboard,
        Err(err) => {
            eprintln!("{LOG_PREFIX} 访问系统剪贴板失败（无 X11 显示？）: {err}");
            return;
        }
    };

    if let Err(err) = clipboard.set().file_list(&paths) {
        eprintln!("{LOG_PREFIX} 写入剪贴板失败: {err}");
    }
}

// --- Quick Look（macOS 专属，Linux 待 P3 内置预览） ---

#[tauri::command]
pub fn toggle_quicklook(items: Vec<serde_json::Value>) {
    let _ = items;
}

#[tauri::command]
pub fn close_quicklook() {}

#[tauri::command]
pub fn update_quicklook(items: Vec<serde_json::Value>) {
    let _ = items;
}

// --- 窗口控制 ---

#[tauri::command]
pub fn hide_main_window(app: AppHandle) {
    hide_main_window_impl(&app);
}

#[tauri::command]
pub fn activate_main_window(app: AppHandle) {
    activate_main_window_impl(&app);
}

#[tauri::command]
pub fn toggle_main_window(app: AppHandle) {
    toggle_main_window_impl(&app);
}

#[tauri::command]
pub fn set_tray_activation_policy(
    app: AppHandle,
    enabled: bool,
    state: State<'_, crate::native_ui::TrayState>,
) -> Result<(), String> {
    state.inner().set_enabled(&app, enabled)
}

// --- 全局快捷键（v1 需 Rust 侧注册；上游是前端 JS 插件） ---

#[tauri::command]
pub fn set_global_shortcut(
    app: AppHandle,
    accelerator: String,
    state: State<'_, std::sync::Arc<IndexHandle>>,
) -> Result<(), String> {
    use tauri::GlobalShortcutManager;

    let mut manager = app.global_shortcut_manager();

    // 启动时 main.rs 已注册同一个快捷键，避免重复注册
    if manager.is_registered(accelerator.as_str()).unwrap_or(false) {
        return Ok(());
    }

    let handle = app.clone();
    manager
        .register(accelerator.as_str(), move || {
            toggle_main_window_impl(&handle)
        })
        .map_err(|err| format!("注册全局快捷键失败: {err}"))?;

    let _ = state; // 目前不需要记录上一次的快捷键；P3 支持自定义时再补
    Ok(())
}

/// 前端启动探针：React 挂载完成后调用一次（冒烟/验收用）。
#[tauri::command]
pub fn frontend_ready(platform: Option<String>) {
    eprintln!(
        "[miaosou] 前端已加载（React 已挂载）{}",
        platform
            .map(|value| format!(" · {value}"))
            .unwrap_or_default()
    );
}

// ---------------------------------------------------------------------------
// 平台相关：打开 / 定位
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_paths_filter_invalid_entries() {
        let dir = std::env::temp_dir().join(format!("miaosou-clip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("report.pdf");
        std::fs::write(&file, b"x").unwrap();

        let accepted = normalize_clipboard_paths(vec![
            file.to_string_lossy().into_owned(),
            "relative/path.pdf".to_string(),
            "  ".to_string(),
            dir.join("missing.pdf").to_string_lossy().into_owned(),
        ]);
        assert_eq!(accepted, vec![file.clone()], "只接受存在且为绝对路径的条目");

        // ~/ 前缀应展开到 $HOME（HOME 由测试环境提供）
        if let Some(home) = home_dir() {
            let tilde = normalize_clipboard_paths(vec!["~/".to_string()]);
            assert_eq!(tilde, vec![home], "~/ 应展开为主目录");
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ancestor_ignore_of_watch_root_is_dropped() {
        let config = build_index_config(
            "/tmp/xc-smoke",
            vec![
                "/usr".to_string(),
                "/tmp".to_string(),
                "/tmp/xc-smoke/drafts".to_string(),
            ],
            Vec::new(),
        )
        .expect("配置应有效");

        // 根的祖先级忽略会让整棵树为空 → 必须剔除
        assert!(
            !config.ignore_paths.contains(&PathBuf::from("/tmp")),
            "祖先级忽略应被剔除: {:?}",
            config.ignore_paths
        );
        // 无关的系统忽略保留
        assert!(config.ignore_paths.contains(&PathBuf::from("/usr")));
        // 根的子目录忽略仍然有效（用户明确要跳过）
        assert!(config
            .ignore_paths
            .contains(&PathBuf::from("/tmp/xc-smoke/drafts")));
    }

    #[test]
    fn watch_config_normalizes_paths() {
        assert!(normalize_path_input("~").is_some());
        assert!(normalize_path_input("~/Documents").is_some());
        assert!(normalize_path_input("/tmp").is_some());
        assert!(normalize_path_input("relative/path").is_none());
        assert!(normalize_path_input("").is_none());
    }
}
