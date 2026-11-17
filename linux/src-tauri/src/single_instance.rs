//! 单实例：Tauri 启动前用 flock 选主，第二个进程通过 Unix socket 唤起首实例。
//!
//! 设计见 `docs/design/2026-11-17-uos-startup-clipboard-tray-single-instance.md`。
//! 约定：
//!   - 锁文件与 socket 都在 `$XDG_RUNTIME_DIR/miaosou/`（不可用时 `/tmp/miaosou-<uid>/`）；
//!   - 目录 0700、socket 0600、lock 0600，仅同 UID 可访问；
//!   - 第二个进程发送 `SHOW\n` 后退出，不启动 Tauri；
//!   - 首实例尚未拿到 AppHandle 时收到 SHOW，会记 pending，attach 后补一次唤起。

use crate::window_controls;
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, ErrorKind, Write},
    os::unix::{
        fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
        io::AsRawFd,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread,
    time::Duration,
};
use tauri::AppHandle;

type HandleSlot = Arc<Mutex<Option<AppHandle>>>;

static APP_HANDLE: OnceLock<HandleSlot> = OnceLock::new();
static PENDING_SHOW: AtomicBool = AtomicBool::new(false);
static GUARD: OnceLock<InstanceGuard> = OnceLock::new();

struct InstanceGuard {
    /// 锁文件必须活到进程退出；drop 后 flock 释放。
    _lock: File,
    socket_path: PathBuf,
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.socket_path);
    }
}

/// 尝试成为首实例。
///
/// 返回 `true` 表示本进程是 Primary，应继续启动 Tauri；
/// 返回 `false` 表示已有实例，本进程应直接退出。
pub fn acquire() -> bool {
    let (dir, lock_path, socket_path) = runtime_paths();
    if let Err(err) = ensure_runtime_dir(&dir) {
        // 运行时目录不可用：宁可退化为多实例，也不要让应用完全起不来。
        eprintln!("[miaosou] 单实例目录不可用（{err}），跳过单实例保护");
        return true;
    }

    let lock = match open_lock_file(&lock_path) {
        Ok(file) => file,
        Err(err) => {
            eprintln!("[miaosou] 打开单实例锁失败（{err}），跳过单实例保护");
            return true;
        }
    };

    let locked = unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    if !locked {
        let err = std::io::Error::last_os_error();
        if err.kind() != ErrorKind::WouldBlock && err.raw_os_error() != Some(libc::EWOULDBLOCK) {
            eprintln!("[miaosou] 单实例 flock 异常（{err}），按已有实例处理");
        }
        request_show(&socket_path);
        return false;
    }

    // Primary：清理崩溃残留 socket，再监听。
    let _ = fs::remove_file(&socket_path);
    let listener = match UnixListener::bind(&socket_path) {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("[miaosou] 绑定单实例 socket 失败（{err}），本次不做唤起");
            let _ = GUARD.set(InstanceGuard {
                _lock: lock,
                socket_path,
            });
            return true;
        }
    };
    if let Err(err) = fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600)) {
        eprintln!("[miaosou] 设置单实例 socket 权限失败: {err}");
    }

    let slot: HandleSlot = Arc::new(Mutex::new(None));
    let _ = APP_HANDLE.set(slot.clone());

    let spawn_result = thread::Builder::new()
        .name("miaosou-single-instance".to_string())
        .spawn(move || listen(listener, slot));
    if let Err(err) = spawn_result {
        eprintln!("[miaosou] 启动单实例监听线程失败: {err}");
    }

    let _ = GUARD.set(InstanceGuard {
        _lock: lock,
        socket_path,
    });
    true
}

/// 首实例拿到 AppHandle 后调用；若启动期间有第二个实例请求，会补一次唤起。
pub fn attach(app: AppHandle) {
    if let Some(slot) = APP_HANDLE.get() {
        if let Ok(mut guard) = slot.lock() {
            *guard = Some(app.clone());
        }
    }
    if PENDING_SHOW.swap(false, Ordering::SeqCst) {
        activate(&app);
    }
}

fn listen(listener: UnixListener, slot: HandleSlot) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else {
            continue;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));

        let mut line = String::new();
        let read = {
            let mut reader = BufReader::new(&stream);
            reader.read_line(&mut line)
        };
        if read.is_err() {
            continue;
        }

        match line.trim() {
            "SHOW" => {
                let handle = slot.lock().ok().and_then(|guard| guard.clone());
                if let Some(app) = handle {
                    activate(&app);
                } else {
                    PENDING_SHOW.store(true, Ordering::SeqCst);
                }
                let _ = stream.write_all(b"OK\n");
            }
            "PING" => {
                let _ = stream.write_all(b"PONG\n");
            }
            _ => {
                let _ = stream.write_all(b"ERR\n");
            }
        }
    }
}

fn activate(app: &AppHandle) {
    let app_for_main = app.clone();
    if let Err(err) = app.run_on_main_thread(move || {
        window_controls::activate_main_window_impl(&app_for_main);
    }) {
        eprintln!("[miaosou] 单实例唤起主窗口失败: {err}");
    }
}

fn request_show(socket_path: &Path) {
    // 首实例可能还在 bind/listen 的窗口里：最多重试 1s。
    for attempt in 0..20 {
        match UnixStream::connect(socket_path) {
            Ok(mut stream) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
                if stream.write_all(b"SHOW\n").is_ok() {
                    let mut reply = String::new();
                    let _ = BufReader::new(&stream).read_line(&mut reply);
                }
                return;
            }
            Err(_) if attempt < 19 => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(err) => {
                eprintln!("[miaosou] 已有实例但无法连接（{err}），本次启动退出");
                return;
            }
        }
    }
}

fn runtime_paths() -> (PathBuf, PathBuf, PathBuf) {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_dir())
        .unwrap_or_else(|| {
            let uid = unsafe { libc::getuid() };
            std::env::temp_dir().join(format!("miaosou-{uid}"))
        });
    let dir = base.join("miaosou");
    let lock = dir.join("instance.lock");
    let socket = dir.join("instance.sock");
    (dir, lock, socket)
}

fn ensure_runtime_dir(dir: &Path) -> std::io::Result<()> {
    if !dir.exists() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
}

fn open_lock_file(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .mode(0o600)
        .open(path)
}

#[cfg(test)]
mod tests {
    use super::runtime_paths;

    #[test]
    fn runtime_paths_are_under_a_runtime_dir() {
        let (dir, lock, socket) = runtime_paths();
        assert!(dir.ends_with("miaosou"));
        assert_eq!(lock, dir.join("instance.lock"));
        assert_eq!(socket, dir.join("instance.sock"));
    }
}
