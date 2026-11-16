//! 用户数据目录与旧版本迁移。
//!
//! 上游用 `app_config_dir()`（= `$XDG_CONFIG_HOME/<identifier>`）存放索引与窗口状态，
//! 于是**换 identifier 就会搬动用户数据**：升级后第一次启动索引全丢、要重新扫盘。
//!
//! 这里改成固定的目录名 [`identity::CONFIG_DIR_NAME`]，identifier 只承担打包/D-Bus 身份；
//! 同时把改名前（`cn.cardinal.xc`）的数据一次性搬过来，见 [`migrate_legacy_config_dir`]。

use crate::identity::{self, LOG_PREFIX};
use std::path::{Path, PathBuf};

/// `$XDG_CONFIG_HOME/miaosou`（默认 `~/.config/miaosou`），并保证目录存在。
pub fn app_config_dir() -> Option<PathBuf> {
    let dir = tauri::api::path::config_dir()?.join(identity::CONFIG_DIR_NAME);
    match std::fs::create_dir_all(&dir) {
        Ok(()) => Some(dir),
        Err(err) => {
            eprintln!("{LOG_PREFIX} 创建配置目录 {dir:?} 失败: {err}");
            None
        }
    }
}

/// 索引文件路径。
///
/// 配置目录不可用时退到临时文件：搜索功能不受影响，只是重启后要重建索引。
/// （上游在这里会往 `$HOME` 写一个隐藏文件，已修正。）
pub fn index_db_path() -> PathBuf {
    if let Some(dir) = app_config_dir() {
        return dir.join(identity::INDEX_DB_FILE_NAME);
    }
    let fallback = std::env::temp_dir().join(format!("{}-index.db", identity::CONFIG_DIR_NAME));
    eprintln!("{LOG_PREFIX} 配置目录不可用，索引改为临时文件 {fallback:?}（重启后需重建）");
    fallback
}

/// 启动时调用一次：把旧版本目录里的索引与窗口状态搬到新目录。
///
/// 幂等：新目录已有索引就什么都不做；搬不动时保留旧目录原样，下次启动再试。
pub fn migrate_legacy_config_dir() {
    let Some(base) = tauri::api::path::config_dir() else {
        return;
    };
    if let Some(legacy_dir) = migrate_in(&base) {
        eprintln!("{LOG_PREFIX} 已从旧目录 {legacy_dir:?} 迁移索引与窗口状态");
    }
}

/// [`migrate_legacy_config_dir`] 的纯函数版本（可单测）：返回实际迁移来源目录。
fn migrate_in(base: &Path) -> Option<PathBuf> {
    let new_dir = base.join(identity::CONFIG_DIR_NAME);
    if new_dir.join(identity::INDEX_DB_FILE_NAME).exists() {
        // 已经迁移过，或者是全新安装。
        return None;
    }

    for legacy_name in identity::LEGACY_CONFIG_DIR_NAMES {
        let legacy_dir = base.join(legacy_name);
        if !legacy_dir
            .join(identity::LEGACY_INDEX_DB_FILE_NAME)
            .exists()
        {
            continue;
        }
        std::fs::create_dir_all(&new_dir).ok()?;

        // 索引：搬到新文件名；窗口状态：同名搬运（搬不动也无所谓，会重建默认几何）。
        if !move_file(
            &legacy_dir.join(identity::LEGACY_INDEX_DB_FILE_NAME),
            &new_dir.join(identity::INDEX_DB_FILE_NAME),
        ) {
            continue;
        }
        let _ = move_file(
            &legacy_dir.join(identity::WINDOW_STATE_FILE_NAME),
            &new_dir.join(identity::WINDOW_STATE_FILE_NAME),
        );
        return Some(legacy_dir);
    }
    None
}

/// 同盘 rename 是原子的；跨设备（少见）退化为 copy + 删源。
fn move_file(from: &Path, to: &Path) -> bool {
    if std::fs::rename(from, to).is_ok() {
        return true;
    }
    match std::fs::copy(from, to) {
        Ok(_) => {
            let _ = std::fs::remove_file(from);
            true
        }
        Err(_) => false,
    }
}

/// 把绝对路径编码成 `file://` URI（unreserved 字符与 `/` 原样，其余百分号编码）。
///
/// 两处使用：freedesktop 缩略图缓存的文件名（`md5(file:// URI)`），
/// 以及 DDE 文件管理器的 `?selectUrl=file://…` 定位参数。
pub(crate) fn file_uri(path: &Path) -> Option<String> {
    let text = path.to_str()?;
    let mut encoded = String::with_capacity(text.len() + 8);
    encoded.push_str("file://");
    for byte in text.bytes() {
        let unreserved =
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/');
        if unreserved {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    Some(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// 每次用唯一的临时根目录，避免并发测试互相干扰。
    fn temp_base(tag: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let base = std::env::temp_dir().join(format!(
            "miaosou-paths-test-{tag}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn migrates_legacy_index_and_window_state() {
        let base = temp_base("migrate");
        let legacy = base.join("cn.cardinal.xc");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("cardinal.db"), b"index-bytes").unwrap();
        std::fs::write(legacy.join(".window-state.json"), b"{}").unwrap();

        let migrated_from = migrate_in(&base).expect("应当迁移旧目录");

        assert_eq!(migrated_from, legacy);
        let new_dir = base.join("miaosou");
        assert_eq!(
            std::fs::read(new_dir.join("index.db")).unwrap(),
            b"index-bytes"
        );
        assert!(new_dir.join(".window-state.json").exists());
        assert!(!legacy.join("cardinal.db").exists());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn skips_when_new_dir_already_has_index() {
        let base = temp_base("already-migrated");
        let new_dir = base.join("miaosou");
        let legacy = base.join("cn.cardinal.xc");
        std::fs::create_dir_all(&new_dir).unwrap();
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(new_dir.join("index.db"), b"new").unwrap();
        std::fs::write(legacy.join("cardinal.db"), b"old").unwrap();

        assert!(migrate_in(&base).is_none());
        // 旧文件保持原样（不做破坏性清理）。
        assert!(legacy.join("cardinal.db").exists());
        assert_eq!(std::fs::read(new_dir.join("index.db")).unwrap(), b"new");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn is_noop_without_legacy_data() {
        let base = temp_base("fresh");
        assert!(migrate_in(&base).is_none());
        let _ = std::fs::remove_dir_all(&base);
    }
}
