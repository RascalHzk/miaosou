use libc::dev_t;
#[cfg(target_os = "macos")]
use objc2_core_services::{FSEventsGetCurrentEventId, FSEventsGetLastEventIdForDeviceBeforeTime};
use std::{collections::HashMap, time::SystemTime};

#[allow(dead_code)]
pub fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

/// macOS 返回当前 FSEvents 事件 id；Linux 没有 FSEvents，返回 0（仅作占位）。
pub fn current_event_id() -> u64 {
    #[cfg(target_os = "macos")]
    unsafe {
        FSEventsGetCurrentEventId()
    }
    #[cfg(not(target_os = "macos"))]
    0
}

/// 仅 macOS 有意义（按设备 + 时间查历史事件 id）。
pub fn last_event_id_before_time(dev: dev_t, timestamp: i64) -> u64 {
    #[cfg(target_os = "macos")]
    {
        let _ = (dev, timestamp);
        unsafe { FSEventsGetLastEventIdForDeviceBeforeTime(dev, timestamp as f64) }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (dev, timestamp);
        0
    }
}

/// Given a device id, an event id, and a cache mapping timestamps to last event ids before them,
/// perform a binary search to find the timestamp corresponding to the event id.
pub fn event_id_to_timestamp(dev: dev_t, event_id: u64, cache: &mut HashMap<i64, u64>) -> i64 {
    #[cfg(not(target_os = "macos"))]
    {
        // Linux 无 FSEvents 历史事件 id 概念；保留签名供跨平台调用方使用。
        let _ = (dev, event_id, cache);
        0
    }

    #[cfg(target_os = "macos")]
    {
        let mut begin = 0i64;
        let mut end = current_timestamp();
        loop {
            let mid = (begin + end) / 2;
            if mid == begin || mid == end {
                return mid;
            }
            let mid_event_id = *cache
                .entry(mid)
                .or_insert_with(|| last_event_id_before_time(dev, mid));
            if mid_event_id < event_id {
                begin = mid
            } else if mid_event_id > event_id {
                end = mid
            } else {
                return mid;
            }
        }
    }
}
