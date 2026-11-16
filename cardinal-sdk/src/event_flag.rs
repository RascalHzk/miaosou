use bitflags::bitflags;

// FSEvents 事件标志位。
//
// 上游直接引用 `objc2_core_services::kFSEventStreamEventFlag*` 常量，
// 但那条依赖只在 macOS 存在（Linux 上不需要 FSEvents）。为了让 search-cache
// 等内核 crate 在 Linux 上也能编译，这里把取值落到本文件（与 macOS
// <CoreServices/CoreServices.h> / FSEvents.h 中的定义一一对应）。
bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct EventFlag: u32 {
        const None = 0x0000_0000;
        const MustScanSubDirs = 0x0000_0001;
        const UserDropped = 0x0000_0002;
        const KernelDropped = 0x0000_0004;
        const EventIdsWrapped = 0x0000_0008;
        const HistoryDone = 0x0000_0010;
        const RootChanged = 0x0000_0020;
        const Mount = 0x0000_0040;
        const Unmount = 0x0000_0080;
        const ItemCreated = 0x0000_0100;
        const ItemRemoved = 0x0000_0200;
        const ItemInodeMetaMod = 0x0000_0400;
        const ItemRenamed = 0x0000_0800;
        const ItemModified = 0x0000_1000;
        const ItemFinderInfoMod = 0x0000_2000;
        const ItemChangeOwner = 0x0000_4000;
        const ItemXattrMod = 0x0000_8000;
        const ItemIsFile = 0x0001_0000;
        const ItemIsDir = 0x0002_0000;
        const ItemIsSymlink = 0x0004_0000;
        const OwnEvent = 0x0008_0000;
        const IsHardlink = 0x0010_0000;
        const IsLastHardlink = 0x0020_0000;
        const Cloned = 0x0040_0000;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    Unknown,
    File,
    Dir,
    Symlink,
    Hardlink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanType {
    /// Scan a single node
    SingleNode,
    /// Scan the whole folder, including sub-folders.
    Folder,
    /// Something wrong happened, do re-indexing.
    /// Should only happen with `kFSEventStreamCreateFlagWatchRoot` set in EventStream::new().
    ReScan,
    /// Do nothing, since event id is always updated.
    Nop,
}

impl EventFlag {
    pub fn event_type(&self) -> EventType {
        if self.contains(EventFlag::IsHardlink) | self.contains(EventFlag::IsLastHardlink) {
            EventType::Hardlink
        } else if self.contains(EventFlag::ItemIsSymlink) {
            EventType::Symlink
        } else if self.contains(EventFlag::ItemIsDir) {
            EventType::Dir
        } else if self.contains(EventFlag::ItemIsFile) {
            EventType::File
        } else {
            EventType::Unknown
        }
    }

    pub fn scan_type(&self) -> ScanType {
        let event_type = self.event_type();
        let is_dir = matches!(event_type, EventType::Dir);
        if self.contains(EventFlag::HistoryDone) | self.contains(EventFlag::EventIdsWrapped) {
            ScanType::Nop
        } else if self.contains(EventFlag::RootChanged) {
            ScanType::ReScan
        } else {
            // Strange event, doesn't know when it happens, processing it using a generic way
            // e.g. new event: fs_event=FsEvent { path: "/.docid/16777229/changed/782/src=0,dst=41985052", flag: kFSEventStreamEventFlagNone, id: 471533015 }
            if is_dir {
                ScanType::Folder
            } else {
                ScanType::SingleNode
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_deduction() {
        assert!(matches!(
            (EventFlag::ItemIsFile).event_type(),
            EventType::File
        ));
        assert!(matches!(
            (EventFlag::ItemIsDir).event_type(),
            EventType::Dir
        ));
        assert!(matches!(
            (EventFlag::ItemIsSymlink).event_type(),
            EventType::Symlink
        ));
        assert!(matches!(
            (EventFlag::IsHardlink).event_type(),
            EventType::Hardlink
        ));
        // Unknown when no type bits set
        assert!(matches!((EventFlag::None).event_type(), EventType::Unknown));
    }

    #[test]
    fn test_scan_type_root_changed_and_history_done() {
        // RootChanged should always trigger ReScan regardless of type bits; test with RootChanged only.
        assert_eq!(EventFlag::RootChanged.scan_type(), ScanType::ReScan);
        assert_eq!(EventFlag::HistoryDone.scan_type(), ScanType::Nop);
    }

    #[test]
    fn test_scan_type_created_removed_modified() {
        // File create => SingleNode
        assert!(matches!(
            (EventFlag::ItemCreated | EventFlag::ItemIsFile).scan_type(),
            ScanType::SingleNode
        ));
        // Dir removal => Folder
        assert!(matches!(
            (EventFlag::ItemRemoved | EventFlag::ItemIsDir).scan_type(),
            ScanType::Folder
        ));
        // File removal => SingleNode
        assert!(matches!(
            (EventFlag::ItemRemoved | EventFlag::ItemIsFile).scan_type(),
            ScanType::SingleNode
        ));
        // File modified => SingleNode
        assert!(matches!(
            (EventFlag::ItemModified | EventFlag::ItemIsFile).scan_type(),
            ScanType::SingleNode
        ));
    }

    #[test]
    fn test_scan_type_must_scan_subdirs() {
        // MustScanSubDirs => Folder
        assert!(matches!(
            (EventFlag::MustScanSubDirs | EventFlag::ItemIsDir).scan_type(),
            ScanType::Folder
        ));
    }
}
