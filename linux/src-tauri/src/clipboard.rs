//! Linux 剪贴板：复制文件时同时提供 DDE/GNOME 需要的 `x-special/gnome-copied-files`
//! 和标准 `text/uri-list`。
//!
//! arboard 3.7 在 Linux 只声明 `text/uri-list`；DDE 文件管理器按
//! `x-special/gnome-copied-files` 识别“复制的文件”，所以这里改用 GTK clipboard。
//! GTK 的 selection 由主循环持有，进程存活期间持续可粘贴。

use crate::paths::file_uri;
use std::{path::PathBuf, sync::Arc};
use tauri::AppHandle;

struct Payload {
    uri_list: Vec<u8>,
    gnome_copied: Vec<u8>,
    plain: Vec<u8>,
}

impl Payload {
    fn new(paths: &[PathBuf]) -> Result<Self, String> {
        let uris: Vec<String> = paths.iter().filter_map(|path| file_uri(path)).collect();
        if uris.is_empty() {
            return Err("没有可编码为 file:// URI 的路径".to_string());
        }

        let plain_paths: Vec<String> = paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();

        Ok(Self {
            uri_list: format!("{}\r\n", uris.join("\r\n")).into_bytes(),
            // Nautilus / GNOME 的格式：动作行 + 每行一个 URI。
            gnome_copied: format!("copy\n{}\n", uris.join("\n")).into_bytes(),
            plain: format!("{}\n", plain_paths.join("\n")).into_bytes(),
        })
    }
}

pub fn copy_files(app: &AppHandle, paths: Vec<PathBuf>) -> Result<(), String> {
    let payload = Arc::new(Payload::new(&paths)?);
    app.run_on_main_thread(move || {
        let clipboard = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
        let targets = [
            gtk::TargetEntry::new("text/uri-list", gtk::TargetFlags::empty(), 0),
            gtk::TargetEntry::new("x-special/gnome-copied-files", gtk::TargetFlags::empty(), 1),
            gtk::TargetEntry::new("text/plain", gtk::TargetFlags::empty(), 2),
        ];

        let data = payload.clone();
        let ok = clipboard.set_with_data(&targets, move |_clipboard, selection, info| {
            let bytes: &[u8] = match info {
                0 => &data.uri_list,
                1 => &data.gnome_copied,
                _ => &data.plain,
            };
            let target = selection.target();
            selection.set(&target, 8, bytes);
        });

        if !ok {
            eprintln!("[miaosou] 写入 GTK 剪贴板失败（set_with_data 返回 false）");
        }
    })
    .map_err(|err| format!("切换到 GTK 主线程失败: {err}"))
}

#[cfg(test)]
mod tests {
    use super::Payload;
    use std::path::PathBuf;

    #[test]
    fn builds_all_three_targets() {
        let paths = vec![
            PathBuf::from("/home/u/hello world.txt"),
            PathBuf::from("/home/u/项目.pdf"),
        ];
        let payload = Payload::new(&paths).expect("payload");

        let uri_list = String::from_utf8(payload.uri_list).unwrap();
        assert!(uri_list.contains("file:///home/u/hello%20world.txt"));
        assert!(uri_list.contains("file:///home/u/%E9%A1%B9%E7%9B%AE.pdf"));
        assert!(uri_list.ends_with("\r\n"));

        let gnome = String::from_utf8(payload.gnome_copied).unwrap();
        assert!(gnome.starts_with("copy\n"));
        assert!(gnome.contains("file:///home/u/hello%20world.txt\n"));

        let plain = String::from_utf8(payload.plain).unwrap();
        assert!(plain.contains("/home/u/hello world.txt"));
    }
}
