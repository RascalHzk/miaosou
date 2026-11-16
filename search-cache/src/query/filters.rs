//! 扩展名 / 类型过滤器辅助（原 query.rs 底部私有函数）。

use crate::SlabIndex;
use cardinal_syntax::{ArgumentKind, FilterArgument};
use fswalk::NodeFileType;
use hashbrown::HashSet;

pub(super) fn normalize_extensions(argument: &FilterArgument) -> HashSet<String> {
    let mut values = HashSet::new();
    match &argument.kind {
        ArgumentKind::List(list) => {
            for item in list {
                if let Some(ext) = normalize_extension(item) {
                    values.insert(ext);
                }
            }
        }
        _ => {
            if let Some(ext) = normalize_extension(&argument.raw) {
                values.insert(ext);
            }
        }
    }
    values
}

pub(super) fn normalize_extension(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_start_matches('.');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_ascii_lowercase())
    }
}

pub(super) fn extension_of(name: &str) -> Option<String> {
    let pos = name.rfind('.')?;
    if pos + 1 >= name.len() {
        return None;
    }
    Some(name[pos + 1..].to_ascii_lowercase())
}

pub(super) fn dedup_indices_in_place(indices: &mut Vec<SlabIndex>) {
    let mut seen = HashSet::with_capacity(indices.len());
    indices.retain(|index| seen.insert(*index));
}

#[derive(Clone, Copy)]
pub(super) enum TypeFilterTarget {
    NodeType(NodeFileType),
    Extensions(&'static [&'static str]),
}

pub(super) fn lookup_type_group(name: &str) -> Option<TypeFilterTarget> {
    match name {
        "file" | "files" => Some(TypeFilterTarget::NodeType(NodeFileType::File)),
        "folder" | "folders" | "dir" | "directory" => {
            Some(TypeFilterTarget::NodeType(NodeFileType::Dir))
        }
        "picture" | "pictures" | "image" | "images" | "photo" | "photos" => {
            Some(TypeFilterTarget::Extensions(PICTURE_EXTENSIONS))
        }
        "video" | "videos" | "movie" | "movies" => {
            Some(TypeFilterTarget::Extensions(VIDEO_EXTENSIONS))
        }
        "audio" | "audios" | "music" | "song" | "songs" => {
            Some(TypeFilterTarget::Extensions(AUDIO_EXTENSIONS))
        }
        "doc" | "docs" | "document" | "documents" | "text" | "office" => {
            Some(TypeFilterTarget::Extensions(DOCUMENT_EXTENSIONS))
        }
        "presentation" | "presentations" | "ppt" | "slides" => {
            Some(TypeFilterTarget::Extensions(PRESENTATION_EXTENSIONS))
        }
        "spreadsheet" | "spreadsheets" | "xls" | "excel" | "sheet" | "sheets" => {
            Some(TypeFilterTarget::Extensions(SPREADSHEET_EXTENSIONS))
        }
        "pdf" => Some(TypeFilterTarget::Extensions(PDF_EXTENSIONS)),
        "archive" | "archives" | "compressed" | "zip" => {
            Some(TypeFilterTarget::Extensions(ARCHIVE_EXTENSIONS))
        }
        "code" | "source" | "dev" => Some(TypeFilterTarget::Extensions(CODE_EXTENSIONS)),
        "exe" | "exec" | "executable" | "executables" | "program" | "programs" | "app" | "apps" => {
            Some(TypeFilterTarget::Extensions(EXECUTABLE_EXTENSIONS))
        }
        _ => None,
    }
}

pub(super) const PICTURE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "tif", "tiff", "webp", "ico", "svg", "heic", "heif", "raw",
    "arw", "cr2", "orf", "raf", "psd", "ai",
];
pub(super) const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "avi", "mkv", "wmv", "webm", "flv", "mpg", "mpeg", "3gp", "3g2", "ts",
    "mts", "m2ts",
];
pub(super) const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "wav", "flac", "aac", "ogg", "oga", "opus", "wma", "m4a", "alac", "aiff",
];
pub(super) const DOCUMENT_EXTENSIONS: &[&str] = &[
    "txt", "md", "rst", "doc", "docx", "rtf", "odt", "pdf", "pages", "rtfd",
];
pub(super) const PRESENTATION_EXTENSIONS: &[&str] = &["ppt", "pptx", "key", "odp"];
pub(super) const SPREADSHEET_EXTENSIONS: &[&str] = &["xls", "xlsx", "csv", "numbers", "ods"];
pub(super) const PDF_EXTENSIONS: &[&str] = &["pdf"];
pub(super) const ARCHIVE_EXTENSIONS: &[&str] = &[
    "zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "xz", "zst", "cab", "iso", "dmg",
];
pub(super) const CODE_EXTENSIONS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "c", "cc", "cpp", "cxx", "h", "hpp", "hh", "java", "cs", "py",
    "go", "rb", "swift", "kt", "kts", "php", "html", "css", "scss", "sass", "less", "json", "yaml",
    "yml", "toml", "ini", "cfg", "sh", "zsh", "fish", "ps1", "psm1", "sql", "lua", "pl", "pm", "r",
    "m", "mm", "dart", "scala", "ex", "exs",
];
pub(super) const EXECUTABLE_EXTENSIONS: &[&str] = &[
    "exe", "msi", "bat", "cmd", "com", "ps1", "psm1", "app", "apk", "ipa", "jar", "bin", "run",
    "pkg",
];
