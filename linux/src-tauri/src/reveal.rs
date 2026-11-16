//! 文件管理器定位 / 默认程序打开（Linux 桌面差异收口）
//!
//! Tauri v1 没有跨平台的 "reveal in file manager" / "open with default app" API
//! （上游 macOS 用 `open -R` / `open`）。Linux 桌面栈分裂，这里按「候选命令列表」
//! 逐个尝试，并且**等待进程结果**再决定是否回退：
//!
//! 定位（reveal）：
//!   1. `dde-file-manager file://<父目录>/?selectUrl=file://<文件>` —— UOS/DDE，
//!      打开父目录并选中文件（DDE 文管用 `?selectUrl=` 查询参数定位，没有 `--show`）
//!   2. `dde-file-manager file://<父目录>/` —— DDE 兜底：只打开父目录
//!   3. `nautilus --select <path>` —— GNOME
//!   4. `thunar --select <path>`   —— XFCE
//!   5. `dolphin --select <path>`  —— KDE
//!   6. `xdg-open <父目录>`        —— 兜底：打开父目录
//!
//! 打开（open）：`dde-open`（UOS 默认程序启动器）→ `xdg-open` → `gio open`。
//!
//! 注意：只把「进程启动失败 / 退出码非 0」当作失败；命令在 3 秒内没退出则视为
//! 已经把 GUI 拉起来了（部分桌面 opener 会常驻）。失败信息会聚合返回给前端。

#[cfg(any(target_os = "linux", test))]
use crate::paths::file_uri;
use std::path::Path;
#[cfg(target_os = "linux")]
use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// 一条待执行的命令（program + args）。
#[cfg(any(target_os = "linux", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPlan {
    pub program: &'static str,
    pub args: Vec<String>,
}

/// 在文件管理器中定位文件：按桌面优先级给出候选命令。
#[cfg(any(target_os = "linux", test))]
pub fn reveal_candidates(path: &Path) -> Vec<CommandPlan> {
    let target = path.to_string_lossy().into_owned();
    let mut plans = Vec::new();

    // DDE：用 file:// URI + ?selectUrl= 定位；父目录 URI 需要以 / 结尾
    if let (Some(parent), Some(parent_uri)) = (path.parent(), path.parent().and_then(file_uri)) {
        if !parent.as_os_str().is_empty() {
            if let Some(file_uri) = file_uri(path) {
                let mut parent_uri = parent_uri;
                if !parent_uri.ends_with('/') {
                    parent_uri.push('/');
                }
                plans.push(CommandPlan {
                    program: "dde-file-manager",
                    args: vec![format!("{parent_uri}?selectUrl={file_uri}")],
                });
            }
            let mut parent_uri =
                file_uri(parent).unwrap_or_else(|| parent.to_string_lossy().into_owned());
            if !parent_uri.ends_with('/') {
                parent_uri.push('/');
            }
            plans.push(CommandPlan {
                program: "dde-file-manager",
                args: vec![parent_uri],
            });
        }
    }

    plans.extend([
        CommandPlan {
            program: "nautilus",
            args: vec!["--select".to_string(), target.clone()],
        },
        CommandPlan {
            program: "thunar",
            args: vec!["--select".to_string(), target.clone()],
        },
        CommandPlan {
            program: "dolphin",
            args: vec!["--select".to_string(), target.clone()],
        },
    ]);

    // 兜底：打开父目录（根目录没有父目录，此时不给兜底项）
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        plans.push(CommandPlan {
            program: "xdg-open",
            args: vec![parent.to_string_lossy().into_owned()],
        });
    }
    plans
}

/// 用默认程序打开路径：UOS 的 dde-open 优先，其次 xdg-open / gio。
#[cfg(any(target_os = "linux", test))]
pub fn open_candidates(path: &Path) -> Vec<CommandPlan> {
    let target = path.to_string_lossy().into_owned();
    vec![
        CommandPlan {
            program: "dde-open",
            args: vec![target.clone()],
        },
        CommandPlan {
            program: "xdg-open",
            args: vec![target.clone()],
        },
        CommandPlan {
            program: "gio",
            args: vec!["open".to_string(), target],
        },
    ]
}

/// 按顺序尝试候选命令，返回首个成功的结果；全部失败时返回聚合错误。
#[cfg(target_os = "linux")]
fn run_candidates(plans: &[CommandPlan]) -> Result<(), String> {
    let mut failures: Vec<String> = Vec::new();
    for plan in plans {
        match run_one(plan) {
            Ok(()) => return Ok(()),
            Err(err) => failures.push(format!("{}: {err}", plan.program)),
        }
    }
    Err(format!(
        "没有可用的打开方式（已尝试 {} 个）: {}",
        plans.len(),
        failures.join("; ")
    ))
}

/// 执行单条命令：等待 3 秒；正常退出码 0 或在跑（GUI 已拉起）算成功。
#[cfg(target_os = "linux")]
fn run_one(plan: &CommandPlan) -> Result<(), String> {
    let mut child = Command::new(plan.program)
        .args(&plan.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| err.to_string())?;

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stderr.take() {
                    let _ = pipe.read_to_string(&mut stderr);
                }
                let detail = stderr.trim();
                return Err(if detail.is_empty() {
                    format!("退出码 {status}")
                } else {
                    format!("退出码 {status}: {detail}")
                });
            }
            Ok(None) if Instant::now() >= deadline => return Ok(()),
            Ok(None) => std::thread::sleep(Duration::from_millis(30)),
            Err(err) => return Err(err.to_string()),
        }
    }
}

/// 在文件管理器中定位路径。
#[cfg(target_os = "linux")]
pub fn reveal(path: &Path) -> Result<(), String> {
    run_candidates(&reveal_candidates(path))
}

/// 用默认程序打开路径。
#[cfg(target_os = "linux")]
pub fn open(path: &Path) -> Result<(), String> {
    run_candidates(&open_candidates(path))
}

/// macOS（仅开发机）：`open -R` / `open`。
#[cfg(target_os = "macos")]
pub fn reveal(path: &Path) -> Result<(), String> {
    std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|err| err.to_string())
}

#[cfg(target_os = "macos")]
pub fn open(path: &Path) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|err| err.to_string())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn reveal(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn open(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn reveal_prefers_dde_select_url_then_other_managers() {
        let plans = reveal_candidates(&PathBuf::from("/home/u/Documents/report.pdf"));
        let programs: Vec<&str> = plans.iter().map(|plan| plan.program).collect();
        assert_eq!(
            programs,
            vec![
                "dde-file-manager",
                "dde-file-manager",
                "nautilus",
                "thunar",
                "dolphin",
                "xdg-open"
            ]
        );
        // DDE 优先：父目录 URI + ?selectUrl=文件 URI（空格要百分号编码）
        assert_eq!(
            plans[0].args,
            vec![
                "file:///home/u/Documents/?selectUrl=file:///home/u/Documents/report.pdf"
                    .to_string()
            ]
        );
        // DDE 兜底：只打开父目录
        assert_eq!(plans[1].args, vec!["file:///home/u/Documents/".to_string()]);
        assert_eq!(
            plans[2].args,
            vec![
                "--select".to_string(),
                "/home/u/Documents/report.pdf".to_string()
            ]
        );
        // 最后兜底：xdg-open 打开父目录
        assert_eq!(plans[5].args, vec!["/home/u/Documents".to_string()]);
    }

    #[test]
    fn reveal_encodes_special_characters_in_dde_url() {
        let plans = reveal_candidates(&PathBuf::from("/home/u/我的 文档/报告.pdf"));
        assert!(plans[0].args[0].contains("%20"), "{:?}", plans[0].args);
        assert!(
            plans[0].args[0].contains("%E6%88%91"),
            "中文应百分号编码: {:?}",
            plans[0].args
        );
        assert!(plans[0].args[0].starts_with("file:///home/u/"));
    }

    #[test]
    fn reveal_on_root_has_no_xdg_fallback() {
        let plans = reveal_candidates(&PathBuf::from("/"));
        assert!(plans.iter().all(|plan| plan.program != "xdg-open"));
    }

    #[test]
    fn open_prefers_dde_open_then_xdg_and_gio() {
        let plans = open_candidates(&PathBuf::from("/tmp/a b.txt"));
        let programs: Vec<&str> = plans.iter().map(|plan| plan.program).collect();
        assert_eq!(programs, vec!["dde-open", "xdg-open", "gio"]);
        assert_eq!(plans[0].args, vec!["/tmp/a b.txt".to_string()]);
        assert_eq!(
            plans[2].args,
            vec!["open".to_string(), "/tmp/a b.txt".to_string()]
        );
    }
}
