//! 产品身份常量与对外环境变量入口。
//!
//! **改名/换域名只改本文件**：壳内其它模块不写死品牌字符串，避免出现
//! 「包装叫 A、窗口标题叫 B、日志前缀叫 C」的半改名状态。
//!
//! 分层原则（见 `PORTING.md` ADR-006）：
//!   - 对外身份（窗口标题、托盘、包名/二进制名、配置目录、环境变量）：用 `秒搜` / `miaosou`；
//!   - 内部代码身份（`cardinal-sdk` / `cardinal-syntax` 等上游 crate 名、索引格式）：保持不动，
//!     以免打断与上游的 cherry-pick。

/// 中文产品名：窗口标题、托盘菜单、应用菜单、`.desktop` 的 `Name[zh_CN]`。
pub const NAME_ZH: &str = "秒搜";
/// 英文产品名：deb 包名/二进制名、非中文文案。
pub const NAME_EN: &str = "Miaosou";
/// 与 `tauri.conf.json` 的 `identifier` 保持一致，仅用于打包与 GTK/D-Bus 身份。
pub const APP_ID: &str = "cn.miaosou.app";
/// 用户配置目录名：`$XDG_CONFIG_HOME/miaosou`（默认 `~/.config/miaosou`）。
///
/// 刻意**不**用 identifier 拼接：identifier 将来换域名时不该搬动用户数据。
pub const CONFIG_DIR_NAME: &str = "miaosou";
/// 持久化索引文件名（位于上面的配置目录内）。
pub const INDEX_DB_FILE_NAME: &str = "index.db";
/// 窗口状态文件名，沿用上游 v2 插件的落盘格式（见 `window_state.rs`）。
pub const WINDOW_STATE_FILE_NAME: &str = ".window-state.json";
/// stderr 日志前缀，运维排查时按此 grep。
pub const LOG_PREFIX: &str = "[miaosou]";

/// 改名前（上游 Cardinal 时期）的配置目录名，用于一次性迁移。
pub const LEGACY_CONFIG_DIR_NAMES: &[&str] = &["cn.cardinal.xc", "Cardinal"];
/// 改名前索引文件名（迁移时改为 [`INDEX_DB_FILE_NAME`]）。
pub const LEGACY_INDEX_DB_FILE_NAME: &str = "cardinal.db";

// ---------------------------------------------------------------------------
// 环境变量
// ---------------------------------------------------------------------------
//
// 这三个是**对外接口**：运维手册、现场验收脚本会直接引用（见 linux/README.md）。
// 改名后新名优先，旧名继续可读一个版本，并在 stderr 提示一次，避免现场脚本当场失效。

pub const ENV_WATCH_ROOT: &str = "MIAOSOU_WATCH_ROOT";
pub const ENV_SELFTEST: &str = "MIAOSOU_SELFTEST";
pub const ENV_TRACE: &str = "MIAOSOU_TRACE";
/// 实时监控不可用时的「周期全量对账」间隔（秒），默认 300。
pub const ENV_RESCAN_INTERVAL_SECS: &str = "MIAOSOU_RESCAN_INTERVAL_SECS";

const LEGACY_ENV_WATCH_ROOT: &str = "CARDINAL_WATCH_ROOT";
const LEGACY_ENV_SELFTEST: &str = "CARDINAL_SELFTEST";
const LEGACY_ENV_TRACE: &str = "CARDINAL_TRACE";

/// 强制覆盖监控根（验收/冒烟用）。
pub fn watch_root_override() -> Option<String> {
    read(ENV_WATCH_ROOT, LEGACY_ENV_WATCH_ROOT).and_then(|value| value.into_string().ok())
}

/// 索引就绪后自动执行一次的查询（验收自检）。
pub fn selftest_query() -> Option<String> {
    read(ENV_SELFTEST, LEGACY_ENV_SELFTEST).and_then(|value| value.into_string().ok())
}

/// 打印每次查询的命中数与耗时。
pub fn trace_enabled() -> bool {
    read(ENV_TRACE, LEGACY_ENV_TRACE).is_some()
}

/// 实时监控降级后的周期对账间隔（秒）。默认 300s；非法值回退默认。
pub fn rescan_interval_secs() -> u64 {
    std::env::var(ENV_RESCAN_INTERVAL_SECS)
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|secs| *secs >= 30)
        .unwrap_or(300)
}

/// 新名优先；仅旧名存在时告警一次，返回旧值。
fn read(current: &str, legacy: &str) -> Option<std::ffi::OsString> {
    if let Some(value) = std::env::var_os(current) {
        return Some(value);
    }
    let value = std::env::var_os(legacy)?;
    warn_deprecated(current, legacy);
    Some(value)
}

fn warn_deprecated(current: &str, legacy: &str) {
    use std::sync::Once;
    static WARNED: Once = Once::new();
    WARNED.call_once(|| {
        eprintln!(
            "{LOG_PREFIX} 环境变量 {legacy} 已弃用，请改用 {current}（旧名将在下一版本移除）"
        );
    });
}
