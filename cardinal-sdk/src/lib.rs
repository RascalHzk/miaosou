//! Cardinal SDK：文件系统事件与索引工具。
//!
//! 上游是 macOS 专属实现（FSEvents）。信创移植把它拆成：
//!   - 平台无关部分：`EventFlag` / `FsEvent` / `ScanType` / 事件 id 工具 —— Linux 可直接用；
//!   - macOS 专属部分：`EventWatcher`（FSEvents 事件流）—— 只在 `cfg(target_os = "macos")` 编译。
//!
//! Linux 侧的文件监控（inotify）适配层在其上层实现，见 `linux/src-tauri`（P2.5）。

mod event;
mod event_flag;
#[cfg(target_os = "macos")]
mod event_stream;
mod utils;

pub use event::FsEvent;
pub use event_flag::{EventFlag, EventType, ScanType};
#[cfg(target_os = "macos")]
pub use event_stream::{EventStream, EventWatcher};
pub use utils::{current_event_id, event_id_to_timestamp, last_event_id_before_time};

/// FSEvents 事件 id。macOS 上是 `FSEventStreamEventId`；
/// Linux 下没有 FSEvents，仅作为不透明 u64 保留类型（inotify 适配层用自增计数）。
pub type FSEventStreamEventId = u64;
