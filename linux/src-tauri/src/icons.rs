//! Linux 文件图标：freedesktop 图标主题查找（替代上游 macOS 的 `fs-icon`）
//!
//! 上游 `fs-icon` 用 `NSWorkspace.iconForFile` + QuickLook 缩略图拿到 PNG，
//! Linux 上没有等价 API，标准做法是走 **freedesktop 图标主题规范**：
//!
//!   1. 由文件名/扩展名推断图标名（`application-pdf`、`text-x-generic`、`folder` …）；
//!   2. 在 XDG 图标目录里按主题/尺寸/分类查找对应的 PNG；
//!   3. 读成字节 → base64 → `data:image/png;base64,...` 交给前端直接当 `<img src>`。
//!
//! 另外优先使用 **freedesktop 缩略图缓存**（`~/.cache/thumbnails/...`）：
//! GIO/文件管理器（含 UOS 的 dde-file-manager）会给图片/视频/PDF 生成缩略图并写入
//! 该缓存，缓存名 = `md5(file:// URI)` + `.png`。命中就用缩略图，未命中回退到图标主题。
//!
//! 说明：
//!   - 只取 PNG：SVG 需要光栅化（resvg/cairo），P2.5 先不做；hicolor/Adwaita/bloom
//!     在常见尺寸上都有 PNG 版本。
//!   - 查找结果按图标名缓存（同一扩展名只扫一次目录），文件内容不缓存（图标很小）。
//!   - 找不到就返回 `None`，前端显示占位图标（与上游行为一致）。

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

/// 尺寸优先级：文件列表行高不大，128 已经足够清晰，优先小图以省内存/带宽。
const PREFERRED_SIZES: &[&str] = &[
    "128x128", "96x96", "64x64", "48x48", "32x32", "256x256", "512x512", "24x24", "22x22", "16x16",
];

/// 主题优先级：信创桌面（UOS/DDE）优先，其次 freedesktop 通用主题。
const PREFERRED_THEMES: &[&str] = &[
    "bloom",
    "deepin",
    "hicolor",
    "Adwaita",
    "breeze",
    "Papirus",
    "elementary",
    "Yaru",
];

const ICON_CATEGORIES: &[&str] = &["mimetypes", "apps", "places"];

/// 扩展名 → freedesktop 图标名（只覆盖高频类型；未命中的走 text-x-generic）。
fn icon_name_for_extension(extension: &str) -> &'static str {
    match extension {
        // 文档
        "pdf" => "application-pdf",
        "doc" | "docx" | "odt" | "rtf" | "pages" => "x-office-document",
        "xls" | "xlsx" | "ods" | "csv" | "numbers" => "x-office-spreadsheet",
        "ppt" | "pptx" | "odp" | "key" => "x-office-presentation",
        "md" | "markdown" => "text-markdown",
        "txt" | "log" | "readme" => "text-x-generic",
        "html" | "htm" | "xhtml" => "text-html",
        "xml" | "json" | "yaml" | "yml" | "toml" | "ini" | "conf" => "text-x-generic",
        "epub" | "mobi" | "azw3" => "application-epub+zip",
        // 代码
        "rs" | "py" | "js" | "jsx" | "ts" | "tsx" | "go" | "c" | "h" | "cc" | "cpp" | "hpp"
        | "java" | "kt" | "swift" | "sh" | "bash" | "zsh" | "fish" | "lua" | "rb" | "php"
        | "sql" | "css" | "scss" | "vue" | "svelte" => "text-x-script",
        // 图片
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tif" | "tiff" | "heic" | "avif"
        | "ico" | "svg" => "image-x-generic",
        // 音频 / 视频
        "mp3" | "flac" | "wav" | "aac" | "ogg" | "m4a" | "wma" | "opus" => "audio-x-generic",
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" | "wmv" | "m4v" => "video-x-generic",
        // 压缩包 / 安装包
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" | "zst" | "lz4" => {
            "package-x-generic"
        }
        "deb" | "rpm" | "appimage" | "flatpak" | "snap" => "application-x-executable",
        "iso" | "img" => "application-x-cd-image",
        // 字体 / 其他
        "ttf" | "otf" | "woff" | "woff2" => "font-x-generic",
        "desktop" => "application-x-desktop",
        _ => "text-x-generic",
    }
}

/// 根据路径推断图标名（目录 / 可执行文件 / 扩展名）。
fn icon_name_for_path(path: &Path) -> &'static str {
    match path.symlink_metadata() {
        Ok(meta) if meta.is_dir() => return "folder",
        Ok(meta) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if meta.permissions().mode() & 0o111 != 0 {
                    return "application-x-executable";
                }
            }
        }
        Err(_) => {}
    }

    match path
        .extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .as_deref()
    {
        Some(extension) if !extension.is_empty() => icon_name_for_extension(extension),
        _ => "text-x-generic",
    }
}

/// XDG 图标基准目录（按优先级）。
fn icon_base_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(data_home).join("icons"));
    } else if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(&home).join(".local/share/icons"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".icons"));
    }
    dirs.push(PathBuf::from("/usr/local/share/icons"));
    dirs.push(PathBuf::from("/usr/share/icons"));

    dirs
}

/// 主题目录列表：优先固定顺序，再把系统里实际存在的其它主题补在后面。
fn theme_dirs() -> Vec<PathBuf> {
    let mut themes: Vec<PathBuf> = Vec::new();
    for base in icon_base_dirs() {
        for theme in PREFERRED_THEMES {
            let dir = base.join(theme);
            if dir.is_dir() {
                themes.push(dir);
            }
        }
    }
    // 兜底：补充各基准目录下的其它主题（例如定制版 bloom 变体）
    for base in icon_base_dirs() {
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let dir = entry.path();
                if dir.is_dir() && !themes.contains(&dir) {
                    themes.push(dir);
                }
            }
        }
    }
    themes
}

/// 查找图标 PNG 的绝对路径（结果按图标名缓存）。
fn find_icon_path(icon_name: &str) -> Option<PathBuf> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<PathBuf>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    if let Ok(guard) = cache.lock() {
        if let Some(cached) = guard.get(icon_name) {
            return cached.clone();
        }
    }

    let mut found: Option<PathBuf> = None;
    'outer: for theme_dir in theme_dirs() {
        for size in PREFERRED_SIZES {
            for category in ICON_CATEGORIES {
                let candidate = theme_dir
                    .join(size)
                    .join(category)
                    .join(format!("{icon_name}.png"));
                if candidate.is_file() {
                    found = Some(candidate);
                    break 'outer;
                }
            }
        }
        // 部分主题把图标放在 scalable/（SVG，本阶段跳过）或顶层
        let top_level = theme_dir.join(format!("{icon_name}.png"));
        if top_level.is_file() {
            found = Some(top_level);
            break;
        }
    }

    if let Ok(mut guard) = cache.lock() {
        guard.insert(icon_name.to_string(), found.clone());
    }
    found
}

/// 由 XDG 缓存目录解析出的缩略图根目录（`~/.cache/thumbnails`）。
fn thumbnail_root() -> Option<PathBuf> {
    if let Some(cache_home) = std::env::var_os("XDG_CACHE_HOME") {
        let path = PathBuf::from(cache_home);
        if path.is_absolute() {
            return Some(path.join("thumbnails"));
        }
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache/thumbnails"))
}

/// 缩略图尺寸目录优先级：文件列表行高不大，normal(128) 足够；大图再回退 large/x-large。
const THUMBNAIL_SIZES: &[&str] = &["normal", "large", "x-large", "xx-large"];

/// 是否值得去查缩略图缓存（图片 / 视频 / PDF 是常见的 GIO 缩略图来源）。
fn has_thumbnail_source(path: &Path) -> bool {
    let Some(extension) = path
        .extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
    else {
        return false;
    };
    matches!(
        extension.as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "bmp"
            | "webp"
            | "tif"
            | "tiff"
            | "heic"
            | "avif"
            | "svg"
            | "mp4"
            | "mkv"
            | "avi"
            | "mov"
            | "webm"
            | "mp3"
            | "flac"
            | "ogg"
            | "pdf"
    )
}

fn md5_hex(text: &str) -> String {
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    hasher.update(text.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// 在指定缩略图根目录下查找缓存文件（可单测，不依赖进程环境）。
fn find_thumbnail_in(root: &Path, path: &Path) -> Option<PathBuf> {
    if !has_thumbnail_source(path) {
        return None;
    }
    let uri = crate::paths::file_uri(path)?;
    let name = format!("{}.png", md5_hex(&uri));
    THUMBNAIL_SIZES
        .iter()
        .map(|size| root.join(size).join(&name))
        .find(|candidate| candidate.is_file())
}

/// 读取 freedesktop 缩略图缓存；命中返回 `data:image/png;base64,...`。
pub fn thumbnail_data_url(path: &Path) -> Option<String> {
    thumbnail_data_url_in(&thumbnail_root()?, path)
}

fn thumbnail_data_url_in(root: &Path, path: &Path) -> Option<String> {
    let thumbnail = find_thumbnail_in(root, path)?;
    let bytes = std::fs::read(thumbnail).ok()?;
    Some(format!("data:image/png;base64,{}", base64_encode(&bytes)))
}

/// 列表行图标：优先缩略图缓存，其次图标主题。
pub fn row_icon_data_url(path: &Path) -> Option<String> {
    thumbnail_data_url(path).or_else(|| icon_data_url(path))
}

/// 生成前端可直接使用的 `data:image/png;base64,...`；找不到图标返回 `None`。
pub fn icon_data_url(path: &Path) -> Option<String> {
    let icon_name = icon_name_for_path(path);
    let icon_path = find_icon_path(icon_name)?;
    let bytes = std::fs::read(&icon_path).ok()?;
    Some(format!("data:image/png;base64,{}", base64_encode(&bytes)))
}

/// 最小 base64 编码（避免为一个函数引入依赖；标准字母表 + padding）。
fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);

    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(TABLE[((triple >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(triple & 0x3f) as usize] as char
        } else {
            '='
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_extensions_to_freedesktop_names() {
        assert_eq!(icon_name_for_extension("pdf"), "application-pdf");
        assert_eq!(icon_name_for_extension("docx"), "x-office-document");
        assert_eq!(icon_name_for_extension("xlsx"), "x-office-spreadsheet");
        assert_eq!(icon_name_for_extension("png"), "image-x-generic");
        assert_eq!(icon_name_for_extension("zip"), "package-x-generic");
        assert_eq!(icon_name_for_extension("rs"), "text-x-script");
        assert_eq!(icon_name_for_extension("unknown-ext"), "text-x-generic");
    }

    #[test]
    fn base64_encoding_matches_reference() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn directory_and_file_icon_names() {
        let dir = std::env::temp_dir().join(format!("miaosou-icons-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("report.pdf");
        std::fs::write(&file, b"x").unwrap();

        assert_eq!(icon_name_for_path(&dir), "folder");
        assert_eq!(icon_name_for_path(&file), "application-pdf");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn file_uri_percent_encodes_special_characters() {
        let uri =
            crate::paths::file_uri(Path::new("/tmp/我的 报告.pdf")).expect("绝对路径应能编码");
        assert!(uri.starts_with("file:///tmp/"));
        assert!(uri.contains("%20"), "空格应编码: {uri}");
        assert!(!uri.contains(' '), "URI 不应包含裸空格: {uri}");
        // 中文按 UTF-8 字节百分号编码
        assert!(uri.contains("%E6%88%91"), "中文应百分号编码: {uri}");
        // unreserved 字符保持原样
        assert!(uri.contains("/tmp/"));
    }

    #[test]
    fn thumbnail_cache_lookup_uses_md5_of_uri() {
        let dir = std::env::temp_dir().join(format!("miaosou-thumb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cache_root = dir.join("thumbnails");
        let image = dir.join("photo 1.png");
        std::fs::write(&image, b"x").unwrap();

        let uri = crate::paths::file_uri(&image).unwrap();
        let name = format!("{}.png", md5_hex(&uri));
        let normal = cache_root.join("normal");
        std::fs::create_dir_all(&normal).unwrap();
        let thumbnail = normal.join(&name);
        std::fs::write(&thumbnail, b"fake-png-bytes").unwrap();

        let found = find_thumbnail_in(&cache_root, &image).expect("应命中缩略图缓存");
        assert_eq!(found, thumbnail);
        let data_url = thumbnail_data_url_in(&cache_root, &image).expect("应生成 data URL");
        assert!(data_url.starts_with("data:image/png;base64,"));
        assert!(data_url.len() > "data:image/png;base64,".len());

        // 非缩略图来源类型不查缓存
        let text = dir.join("notes.txt");
        std::fs::write(&text, b"x").unwrap();
        assert!(find_thumbnail_in(&cache_root, &text).is_none());

        // 缓存未命中 → None
        let other_image = dir.join("other.png");
        std::fs::write(&other_image, b"x").unwrap();
        assert!(find_thumbnail_in(&cache_root, &other_image).is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// 有图标主题的系统上应能找到 text-x-generic（macOS 开发机没有则跳过）。
    #[test]
    fn finds_icon_on_systems_with_icon_themes() {
        if !Path::new("/usr/share/icons").exists() {
            eprintln!("跳过：当前系统没有 freedesktop 图标主题目录");
            return;
        }
        let Some(found) = find_icon_path("text-x-generic") else {
            // GitHub 的 ubuntu 最小镜像可能只有 hicolor 主题、没有 text-x-generic；
            // UOS/完整桌面环境仍会走到下面的真实断言。
            eprintln!("跳过：当前图标主题未提供 text-x-generic");
            return;
        };
        assert!(found.is_file(), "返回的图标路径应存在");
        let data_url = icon_data_url(Path::new("/etc/hostname"));
        assert!(
            data_url
                .as_deref()
                .map(|url| url.starts_with("data:image/png;base64,"))
                .unwrap_or(false),
            "有 text-x-generic 时应生成 PNG data URL"
        );
    }
}
