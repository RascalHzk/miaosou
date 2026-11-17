//! 文件系统实时监控（P2.5）
//!
//! 把 `notify` 的原始事件翻译成 `search-cache` 能消费的增量更新，替代上游 macOS 版
//! 的 FSEvents 事件流（`cardinal-sdk` 的 `EventWatcher`，我们在 Linux 上不编译它）。
//!
//! ## 设计要点
//!
//! 1. **只监听需要索引的目录**
//!    自己遍历目录树（复用 `fswalk::should_ignore_path` 的忽略/包含规则），给每个目录
//!    注册**一个非递归 watch**。刻意不用 `RecursiveMode::Recursive`：notify 的递归模式
//!    会把 `~/.cache`、`node_modules` 之类也全部挂上 watch，几万个 watch 直接把
//!    `fs.inotify.max_user_watches`（UOS 默认 8192）打爆。
//!
//! 2. **额度保护 + 部分覆盖**
//!    读 `/proc/sys/fs/inotify/max_user_watches`，最多用 80%。目录数超预算或某个目录
//!    注册失败（ENOSPC）时，**保留已经装好的 watch**，把剩余目录交给「每 N 秒全量对账」
//!    （N 见 `MIAOSOU_RESCAN_INTERVAL_SECS`）兜底——实时监控仍是主要更新机制，
//!    只有覆盖不到的部分才依赖对账；完全无法监控时才退化为纯周期对账。
//!
//! 3. **去抖合并**
//!    事件先入集合，300ms 静默后统一批量应用。`git clone`、解压、编译会在极短时间内
//!    产生上万事件，逐条增量写索引既慢又容易抖动。
//!
//! 4. **增量语义**
//!    `search-cache::handle_fs_events` 内部对「受影响路径」做递归重扫：
//!    路径不存在 → 删除节点；存在 → 重建子树。所以这里只需要给出路径 + 文件/目录标记，
//!    不必精确区分 create/modify/rename（rename 的 From/To 两个路径都会被重扫）。
//!
//! 5. **新增目录补挂 watch**
//!    新建目录（含其下已有子树）要立刻补挂 watch，否则后续子事件收不到。
//!
//! 6. **事件风暴熔断**
//!    单批路径数超过阈值 → 直接全量重建：比逐条增量便宜，且结果一定一致。

use crate::{
    identity::{self, LOG_PREFIX},
    index::{IndexConfig, IndexHandle, DEFAULT_IGNORE_DIR_NAMES},
};
use cardinal_sdk::{EventFlag, FsEvent};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tauri::AppHandle;

/// 事件静默窗口：超过这个时间没有新事件就批量应用。
const DEBOUNCE: Duration = Duration::from_millis(300);
/// 事件循环轮询间隔（同时用于响应退出信号）。
const POLL_INTERVAL: Duration = Duration::from_millis(100);
/// 单批路径上限，超过则全量重建。
const MAX_BATCH_PATHS: usize = 2_000;
/// 默认的 inotify watch 上限（读不到 /proc 时使用）。
const FALLBACK_WATCH_LIMIT: usize = 8_192;

/// 一个监控线程的句柄；`stop()` 会等到线程真正退出。
pub struct FsWatcher {
    stop: Sender<()>,
    join: Option<JoinHandle<()>>,
}

impl FsWatcher {
    /// 启动监控。返回 `None` 表示线程都没能创建（极端情况）。
    pub fn start(
        index: Arc<IndexHandle>,
        app: Option<AppHandle>,
        config: IndexConfig,
    ) -> Option<Self> {
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel::<()>();
        let join = match thread::Builder::new()
            .name("miaosou-fs-watch".to_string())
            .spawn(move || run(index, app, config, stop_rx, ready_tx))
        {
            Ok(join) => join,
            Err(err) => {
                eprintln!("{LOG_PREFIX} 启动文件监控线程失败: {err}");
                return None;
            }
        };

        // 等初始 watch 安装完（或降级决定完成）再返回：否则调用方紧接着的写入可能落在
        // 「目录还没挂上 watch」的窗口里，丢事件（inotify/kqueue 都没有历史回放）。
        // 只等 500ms：大目录树的后台安装不应阻塞启动；调用方已在 blocking 线程里。
        if ready_rx.recv_timeout(Duration::from_millis(500)).is_err() {
            eprintln!("{LOG_PREFIX} 等待文件监控就绪超时，继续按异步方式运行");
        }

        Some(Self {
            stop: stop_tx,
            join: Some(join),
        })
    }

    /// 停止并等待线程退出（退出前调用，避免和索引落盘并发）。
    pub fn stop(mut self) {
        let _ = self.stop.send(());
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// 监控路径布局。
///
/// 必须用**规范化（canonical）路径**做前缀匹配：macOS 的 `/var` 是 `/private/var` 的
/// 符号链接，内核事件里给的是规范化路径，直接用用户输入的根路径做 `starts_with`
/// 会把所有事件都过滤掉（Linux 上 `/tmp`、软链家目录同理）。
struct WatchLayout {
    root: PathBuf,
    ignore_paths: Vec<PathBuf>,
    include_paths: Vec<PathBuf>,
    ignore_dir_names: &'static [&'static str],
}

impl WatchLayout {
    fn canonicalized(config: &IndexConfig) -> Self {
        let canonical =
            |path: &PathBuf| std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        Self {
            root: canonical(&config.watch_root),
            ignore_paths: config.ignore_paths.iter().map(canonical).collect(),
            include_paths: config.include_paths.iter().map(canonical).collect(),
            ignore_dir_names: DEFAULT_IGNORE_DIR_NAMES,
        }
    }

    fn should_ignore(&self, path: &Path) -> bool {
        // 用户可以把监控根设成任意目录（包括名为 target/node_modules 的目录），
        // 根目录本身永远不能被“按目录名忽略”排除，否则整个实时监控会退化为对账。
        let name_ignored = path != self.root.as_path()
            && fswalk::should_ignore_dir_name(path, &self.include_paths, self.ignore_dir_names);
        fswalk::should_ignore_path(path, &self.ignore_paths, &self.include_paths) || name_ignored
    }
}

/// 当前可用的 watch 预算：inotify 上限的 80%。
fn watch_budget() -> usize {
    let limit = std::fs::read_to_string("/proc/sys/fs/inotify/max_user_watches")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .unwrap_or(FALLBACK_WATCH_LIMIT);
    (limit * 8 / 10).max(256)
}

fn run(
    index: Arc<IndexHandle>,
    app: Option<AppHandle>,
    config: IndexConfig,
    stop_rx: Receiver<()>,
    ready: Sender<()>,
) {
    // --- 1) 建立 watcher + 初始目录 watch 集 ---
    let (event_tx, event_rx) = mpsc::channel::<notify::Event>();
    let mut watcher = match RecommendedWatcher::new(
        move |result: notify::Result<notify::Event>| {
            if let Ok(event) = result {
                let _ = event_tx.send(event);
            }
        },
        notify::Config::default(),
    ) {
        Ok(watcher) => watcher,
        Err(err) => {
            let _ = ready.send(());
            fallback_periodic(index, app, stop_rx, format!("创建文件监控失败: {err}"));
            return;
        }
    };

    // 事件路径是规范化的；这里把根/忽略表也规范化，保证前缀与忽略判断一致
    let layout = WatchLayout::canonicalized(&config);
    let budget = watch_budget();
    let mut watched: HashSet<PathBuf> = HashSet::new();
    let setup = install_initial_watches(&mut watcher, &layout, budget, &mut watched);
    if setup.installed == 0 {
        drop(watcher);
        let _ = ready.send(());
        fallback_periodic(
            index,
            app,
            stop_rx,
            setup
                .note
                .unwrap_or_else(|| "没有可监控的目录，实时监控不可用".to_string()),
        );
        return;
    }

    let partial = !setup.complete;
    if partial {
        eprintln!(
            "{LOG_PREFIX} 实时监控覆盖 {} 个目录（预算 {}，未覆盖全部）：{}；未覆盖目录依赖每 {}s 全量对账兜底",
            setup.installed,
            budget,
            setup.note.as_deref().unwrap_or("部分目录未能注册 watch"),
            identity::rescan_interval_secs()
        );
    } else {
        eprintln!(
            "{LOG_PREFIX} 实时监控已启动：{} 个目录（预算 {budget}）",
            watched.len()
        );
    }
    let _ = ready.send(());

    // --- 2) 事件循环：收集 → 去抖 → 批量应用 ---
    let mut pending: HashSet<PathBuf> = HashSet::new();
    let mut last_event_at = Instant::now();
    let mut event_id = index.last_event_id().saturating_add(1);
    // 只有「部分覆盖」时才启用周期对账；覆盖完整时 inotify 就是唯一更新来源。
    let reconcile_interval = Duration::from_secs(identity::rescan_interval_secs());
    let mut last_reconcile = Instant::now();

    loop {
        if stop_rx.try_recv().is_ok() {
            break;
        }

        match event_rx.recv_timeout(POLL_INTERVAL) {
            Ok(event) => {
                collect_paths(&event, &layout, &mut pending);
                last_event_at = Instant::now();
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        if !pending.is_empty() && last_event_at.elapsed() >= DEBOUNCE {
            flush(
                &index,
                app.as_ref(),
                &layout,
                &mut pending,
                &mut watched,
                &mut watcher,
                &mut event_id,
            );
        }

        if partial && last_reconcile.elapsed() >= reconcile_interval {
            last_reconcile = Instant::now();
            eprintln!("{LOG_PREFIX} 周期对账：实时监控未覆盖全部目录，触发一次全量重建");
            if let Some(app) = app.as_ref() {
                index.force_rebuild(app);
            }
        }
    }
}

/// 初始建 watch 的结果：装了哪些、是否覆盖完整、失败/超预算原因。
struct WatchSetup {
    installed: usize,
    complete: bool,
    note: Option<String>,
}

/// 初始建 watch：遍历目录树，跳过忽略路径。
///
/// 预算不足或某个目录注册失败时**不放弃已装好的 watch**：保留覆盖到的目录，
/// 让实时监控继续作为主机制，未覆盖部分由调用方的周期对账兜底。
fn install_initial_watches(
    watcher: &mut RecommendedWatcher,
    layout: &WatchLayout,
    budget: usize,
    watched: &mut HashSet<PathBuf>,
) -> WatchSetup {
    // 广度优先：预算有限时优先覆盖浅层目录（最常被访问的区域），
    // 而不是深度优先扎进某一个子树、把其它顶层目录排除在外。
    let mut queue: std::collections::VecDeque<PathBuf> = std::collections::VecDeque::new();
    queue.push_back(layout.root.clone());
    let mut note: Option<String> = None;
    let mut complete = true;

    while let Some(dir) = queue.pop_front() {
        if layout.should_ignore(&dir) {
            continue;
        }
        if watched.contains(&dir) {
            continue;
        }
        if watched.len() >= budget {
            complete = false;
            note = Some(format!(
                "目录数超过 inotify watch 预算 {budget}（可用 fs.inotify.max_user_watches 调整）"
            ));
            break;
        }
        if let Err(err) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
            complete = false;
            note = Some(format!("注册目录 {dir:?} 的 watch 失败: {err}"));
            break;
        }
        watched.insert(dir.clone());

        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                // 不跟随符号链接，避免环路
                if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                    queue.push_back(entry.path());
                }
            }
        }
    }

    WatchSetup {
        installed: watched.len(),
        complete,
        note,
    }
}

/// 把一批事件里的受影响路径收进 pending（过滤掉忽略路径与监控根之外的路径）。
fn collect_paths(event: &notify::Event, layout: &WatchLayout, pending: &mut HashSet<PathBuf>) {
    if identity::trace_enabled() {
        eprintln!("{LOG_PREFIX} 原始事件 {:?}: {:?}", event.kind, event.paths);
    }
    for path in &event.paths {
        if !path.starts_with(&layout.root) {
            continue;
        }
        // 监控根自身的事件要丢弃：search-cache 把「根路径变化」当作必须全量重扫的信号，
        // 会吞掉同一批里的增量（而根目录自身的元数据变化在任何子项变更时都会出现）。
        if path.as_path() == layout.root.as_path() {
            continue;
        }
        if layout.should_ignore(path) {
            continue;
        }
        pending.insert(path.clone());
    }
}

/// 批量应用：补挂新目录 → 翻译成 FsEvent → 交给 search-cache 增量更新。
#[allow(clippy::too_many_arguments)]
fn flush(
    index: &Arc<IndexHandle>,
    app: Option<&AppHandle>,
    layout: &WatchLayout,
    pending: &mut HashSet<PathBuf>,
    watched: &mut HashSet<PathBuf>,
    watcher: &mut RecommendedWatcher,
    event_id: &mut u64,
) {
    let paths: Vec<PathBuf> = pending.drain().collect();
    if paths.is_empty() {
        return;
    }

    // 事件风暴熔断：全量重建比逐条增量更便宜、也更一致
    if paths.len() > MAX_BATCH_PATHS {
        eprintln!(
            "{LOG_PREFIX} 单批文件事件过多（{} 个路径），改为全量重建索引",
            paths.len()
        );
        if let Some(app) = app {
            index.force_rebuild(app);
        }
        return;
    }

    // 新建目录要立刻补挂 watch（含其下已有子树），否则后续子事件收不到
    let budget = watch_budget();
    for path in &paths {
        if path.is_dir() {
            add_watch_tree(path, layout, watched, watcher, budget);
        }
    }

    let events: Vec<FsEvent> = paths
        .iter()
        .map(|path| {
            *event_id = event_id.saturating_add(1);
            FsEvent {
                path: path.clone(),
                flag: event_flag_for(path),
                id: *event_id,
            }
        })
        .collect();

    let processed = events.len();
    if let Err(err) = index.apply_fs_events(events, app) {
        eprintln!("{LOG_PREFIX} 应用文件系统事件失败: {err}");
        return;
    }

    if identity::trace_enabled() {
        eprintln!("{LOG_PREFIX} 增量更新：{processed} 个路径已重扫");
    }
}

/// 新建目录补挂 watch（含其下已有子目录，受预算约束）。
fn add_watch_tree(
    root: &Path,
    layout: &WatchLayout,
    watched: &mut HashSet<PathBuf>,
    watcher: &mut RecommendedWatcher,
    budget: usize,
) {
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        if watched.len() >= budget || watched.contains(&dir) {
            return;
        }
        if layout.should_ignore(&dir) {
            continue;
        }
        if watcher.watch(&dir, RecursiveMode::NonRecursive).is_err() {
            return;
        }
        watched.insert(dir.clone());

        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                    stack.push(entry.path());
                }
            }
        }
    }
}

/// 受影响路径对应的 FSEvents 标志位。
///
/// `search-cache` 只用它判断「目录还是文件、是否要全量重扫」；
/// 路径已不存在时按文件处理即可（内部会走删除分支）。
fn event_flag_for(path: &Path) -> EventFlag {
    match path.symlink_metadata() {
        Ok(meta) if meta.is_dir() => EventFlag::ItemModified | EventFlag::ItemIsDir,
        Ok(_) => EventFlag::ItemModified | EventFlag::ItemIsFile,
        Err(_) => EventFlag::ItemModified,
    }
}

/// 降级路径：实时监控不可用时，按固定间隔做全量对账。
fn fallback_periodic(
    index: Arc<IndexHandle>,
    app: Option<AppHandle>,
    stop_rx: Receiver<()>,
    reason: String,
) {
    let interval_secs = identity::rescan_interval_secs();
    eprintln!(
        "{LOG_PREFIX} {reason}；实时监控不可用，降级为每 {interval_secs}s 全量对账。\
         如需实时索引：sudo sysctl -w fs.inotify.max_user_watches=524288 \
         （并写入 /etc/sysctl.d/99-miaosou.conf）"
    );

    let Some(app) = app else {
        // 测试/无窗口场景：没有 AppHandle 就无法触发重建，直接退出
        return;
    };

    loop {
        match stop_rx.recv_timeout(Duration::from_secs(interval_secs)) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                eprintln!("{LOG_PREFIX} 周期对账：开始重建索引");
                index.force_rebuild(&app);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_budget_is_positive_and_below_limit() {
        let budget = watch_budget();
        assert!(budget >= 256, "预算不应过小: {budget}");
        if let Ok(raw) = std::fs::read_to_string("/proc/sys/fs/inotify/max_user_watches") {
            let limit: usize = raw.trim().parse().unwrap();
            assert!(
                budget <= limit,
                "预算不应超过 inotify 上限: {budget} > {limit}"
            );
        }
    }

    #[test]
    fn event_flag_marks_directory_and_file() {
        let dir = std::env::temp_dir().join(format!("miaosou-watch-flag-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.txt");
        std::fs::write(&file, b"x").unwrap();

        assert!(event_flag_for(&dir).contains(EventFlag::ItemIsDir));
        assert!(event_flag_for(&file).contains(EventFlag::ItemIsFile));
        assert_eq!(
            event_flag_for(&dir.join("missing")),
            EventFlag::ItemModified
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// 直接验证 notify 本平台后端能否投递事件（诊断用，默认 ignored）。
    #[test]
    #[ignore = "诊断 notify 后端，手动运行"]
    fn notify_backend_smoke() {
        use notify::{RecursiveMode, Watcher};
        let dir = std::env::temp_dir().join(format!("miaosou-notify-smoke-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, rx) = mpsc::channel();
        let mut watcher = RecommendedWatcher::new(
            move |result: notify::Result<notify::Event>| {
                if let Ok(event) = result {
                    let _ = tx.send(event);
                }
            },
            notify::Config::default(),
        )
        .expect("watcher 创建失败");
        watcher
            .watch(&dir, RecursiveMode::NonRecursive)
            .expect("watch 失败");
        std::fs::write(dir.join("new-file.txt"), b"x").unwrap();
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(event) => eprintln!("notify 冒烟成功: {:?} {:?}", event.kind, event.paths),
            Err(err) => panic!("notify 冒烟失败，5s 内无事件: {err}"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn partial_watch_keeps_installed_dirs_when_budget_is_small() {
        let root =
            std::env::temp_dir().join(format!("miaosou-watch-partial-{}", std::process::id()));
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::fs::create_dir_all(root.join("b")).unwrap();

        let config = IndexConfig::new(root.clone(), Vec::new(), Vec::new());
        let layout = WatchLayout::canonicalized(&config);
        let mut watcher =
            RecommendedWatcher::new(|_| {}, notify::Config::default()).expect("watcher 应能创建");
        let mut watched = HashSet::new();

        let setup = install_initial_watches(&mut watcher, &layout, 2, &mut watched);

        assert_eq!(setup.installed, 2, "预算内已装好的 watch 不应被丢弃");
        assert_eq!(watched.len(), 2);
        assert!(!setup.complete, "目录数超预算时应标记为部分覆盖");
        assert!(setup.note.unwrap_or_default().contains("预算"));

        std::fs::remove_dir_all(&root).ok();
    }

    /// 真实事件流集成测试（需要 inotify/FSEvents，默认 ignored）。
    ///
    /// 手动运行：`cargo test --features custom-protocol watch_applies_new_files -- --ignored`
    #[test]
    #[ignore = "需要真实文件系统事件，手动运行"]
    fn watch_applies_new_files() {
        let root = std::env::temp_dir().join(format!(
            "miaosou-watch-e2e-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("existing.txt"), b"x").unwrap();

        let config = IndexConfig::new(root.clone(), Vec::new(), Vec::new());
        let index = IndexHandle::new(config.clone(), root.join("index.db"));
        index.install_cache_for_test(
            search_cache::SearchCache::walk_fs_with_ignore(&root, &[]),
            config.clone(),
        );

        let watcher = FsWatcher::start(index.clone(), None, config).expect("监控应能启动");

        // 新建文件 → 应在去抖窗口后出现在索引里
        std::fs::write(root.join("brand-new-report.pdf"), b"x").unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let outcome = index
                .search(None, Some("brand-new".to_string()), false)
                .expect("搜索应成功");
            let hits = outcome.nodes.unwrap_or_default();
            if identity::trace_enabled() {
                eprintln!(
                    "{LOG_PREFIX} 等待增量生效：total={} hits={}",
                    index.total_files(),
                    hits.len()
                );
            }
            if !hits.is_empty() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "5s 内未观察到新增文件（监控事件未生效）"
            );
            thread::sleep(Duration::from_millis(100));
        }

        watcher.stop();
        std::fs::remove_dir_all(&root).ok();
    }
    #[test]
    fn watch_root_is_never_ignored_by_dir_name() {
        let base =
            std::env::temp_dir().join(format!("miaosou-watch-root-name-{}", std::process::id()));
        let root = base.join("target");
        std::fs::create_dir_all(&root).unwrap();

        let config = IndexConfig::new(root.clone(), Vec::new(), Vec::new());
        let layout = WatchLayout::canonicalized(&config);
        assert!(
            !layout.should_ignore(&layout.root),
            "监控根即使叫 target 也不能被目录名规则排除"
        );
        assert!(
            layout.should_ignore(&layout.root.join("nested/target")),
            "根下的 target 子目录仍应被忽略"
        );

        std::fs::remove_dir_all(&base).ok();
    }
}
