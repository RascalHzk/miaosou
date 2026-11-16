//! 真实索引管线（P2）：`fswalk` 全量扫描 + `search-cache`（slab-mmap 持久化）
//!
//! 与上游 `cardinal/src-tauri/src/background.rs` 的关系：
//!   - 复用同一套内核：`WalkData` 扫描、`SearchCache` 建索引/查询、`flush_to_file` 落盘；
//!   - 区别：上游用「后台线程 + mpsc 通道」串行化所有请求（因为它还要处理 FSEvents 事件流）；
//!     信创版 P2 先从简：索引放在 `Mutex<Option<SearchCache>>` 里，查询直接加锁，
//!     构建/重建在独立线程里建好新索引后整体替换（构建期间旧索引仍可查）。
//!
//! 这是一个有意为之的阶段性简化：
//!   - 优点：没有隐藏的线程消息协议，`search` 就能直接用上真实内核；
//!   - 代价：搜索会持锁（单条查询 10~50ms 级），与「构建完成替换」不冲突；
//!     P2.5 接入 inotify 事件流后，再按需要演进成上游那种通道模型。
//!
//! 持久化：索引写到 `$XDG_CONFIG_HOME/miaosou/index.db`（zstd 压缩的 slab，
//! 目录与文件名见 `paths.rs`），格式与上游 macOS 版一致；退出时 `flush`。

use crate::{
    identity::{self, LOG_PREFIX},
    lifecycle::{self, AppLifecycleState},
};
use cardinal_sdk::FsEvent;
use search_cache::{
    HandleFSEError, SearchCache, SearchOptions, SearchOutcome, SearchQuery, SearchResultNode,
    SlabIndex, WalkData,
};
use search_cancel::CancellationToken;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};

pub const EVENT_STATUS_BAR: &str = "status_bar_update";
pub const EVENT_INDEX_READY: &str = "index_ready";
const STATUS_EMIT_INTERVAL: Duration = Duration::from_millis(200);

/// 索引配置：监控根 + 忽略/包含路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexConfig {
    pub watch_root: PathBuf,
    pub ignore_paths: Vec<PathBuf>,
    pub include_paths: Vec<PathBuf>,
}

impl IndexConfig {
    pub fn new(
        watch_root: PathBuf,
        ignore_paths: Vec<PathBuf>,
        include_paths: Vec<PathBuf>,
    ) -> Self {
        Self {
            watch_root,
            ignore_paths,
            include_paths,
        }
    }
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

pub fn default_watch_root() -> PathBuf {
    home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

/// 信创环境默认忽略表（替代上游 38 条 macOS 路径）。
///
/// 原则：系统伪文件系统 + 高频缓存/日志 + 不可变系统目录 + 包管理器缓存。
/// 用户可通过偏好设置覆盖；这里只决定「首次启动」。
pub fn default_ignore_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = [
        "/proc",
        "/sys",
        "/dev",
        "/run",
        "/boot",
        "/lost+found",
        "/var/cache",
        "/var/lib/docker",
        "/var/lib/containers",
        "/var/log",
        "/var/tmp",
        "/snap",
        "/usr",
        "/lib",
        "/lib64",
        "/tmp",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();

    if let Some(home) = home_dir() {
        for relative in [
            ".cache",
            ".npm",
            ".pnpm-store",
            ".cargo",
            ".m2",
            ".gradle",
            ".nuget",
            ".local/share/Trash",
            ".Trash",
            ".dsh",
        ] {
            paths.push(home.join(relative));
        }
    }

    paths
}

pub struct IndexHandle {
    /// 当前可用索引；`None` 表示尚未构建完成。
    cache: Mutex<Option<SearchCache>>,
    /// 当前索引对应的配置（用于判断是否需要重建）。
    loaded_config: Mutex<Option<IndexConfig>>,
    /// 期望配置（最近一次请求的配置）。
    config: Mutex<IndexConfig>,
    db_path: PathBuf,
    building: AtomicBool,
    /// 实时监控线程（构建/加载成功后启动，配置变更或退出时停止）。
    watcher: Mutex<Option<crate::watcher::FsWatcher>>,
    /// 进程正在退出：之后不再启动新的监控线程。
    shutting_down: AtomicBool,
}

impl IndexHandle {
    pub fn new(config: IndexConfig, db_path: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            cache: Mutex::new(None),
            loaded_config: Mutex::new(None),
            config: Mutex::new(config),
            db_path,
            building: AtomicBool::new(false),
            watcher: Mutex::new(None),
            shutting_down: AtomicBool::new(false),
        })
    }

    pub fn config(&self) -> IndexConfig {
        self.config
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_else(|_| {
                IndexConfig::new(default_watch_root(), default_ignore_paths(), vec![])
            })
    }

    pub fn is_ready(&self) -> bool {
        self.cache
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }

    pub fn is_building(&self) -> bool {
        self.building.load(Ordering::SeqCst)
    }

    pub fn total_files(&self) -> usize {
        self.cache
            .lock()
            .map(|guard| {
                guard
                    .as_ref()
                    .map(SearchCache::get_total_files)
                    .unwrap_or(0)
            })
            .unwrap_or(0)
    }

    /// 当前索引最后应用的 FSEvents 事件 id（Linux 上为自增计数）。
    pub fn last_event_id(&self) -> u64 {
        self.cache
            .lock()
            .ok()
            .and_then(|mut guard| guard.as_mut().map(|cache| cache.last_event_id()))
            .unwrap_or(0)
    }

    /// 应用一批文件系统事件（增量更新）。
    ///
    /// `search-cache` 内部会按路径递归重扫；若事件要求全量重建（根目录变化等），
    /// 这里直接触发重建。`app` 为 `None` 时只在测试中使用（不触发重建/不发事件）。
    pub fn apply_fs_events(
        self: &Arc<Self>,
        events: Vec<FsEvent>,
        app: Option<&AppHandle>,
    ) -> Result<(), String> {
        let mut needs_rescan = false;
        {
            let mut guard = self.cache.lock().map_err(|_| "索引锁中毒".to_string())?;
            let Some(cache) = guard.as_mut() else {
                return Ok(());
            };
            match cache.handle_fs_events(events) {
                Ok(()) => {}
                Err(HandleFSEError::Rescan) => needs_rescan = true,
            }
        }

        if needs_rescan {
            eprintln!("{LOG_PREFIX} 监控事件要求全量重建索引");
            if let Some(app) = app {
                self.force_rebuild(app);
            }
            return Ok(());
        }

        if let Some(app) = app {
            emit_status_bar(app, self.total_files());
        }
        Ok(())
    }

    /// 停止旧监控并启动新的（配置变更/索引重建后调用）。
    pub fn restart_watcher(self: &Arc<Self>, app: Option<&AppHandle>, config: &IndexConfig) {
        if self.shutting_down.load(Ordering::SeqCst) {
            return;
        }
        self.shutdown_watcher();
        match crate::watcher::FsWatcher::start(self.clone(), app.cloned(), config.clone()) {
            Some(watcher) => {
                if let Ok(mut guard) = self.watcher.lock() {
                    *guard = Some(watcher);
                }
            }
            None => eprintln!("{LOG_PREFIX} 实时监控启动失败，索引只在启动/手动重建时更新"),
        }
    }

    /// 停止监控线程（幂等；退出前调用）。
    pub fn shutdown_watcher(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        let old = self.watcher.lock().ok().and_then(|mut guard| guard.take());
        if let Some(watcher) = old {
            watcher.stop();
        }
    }

    /// 是否正在实时监控（诊断/测试用）。
    pub fn has_watcher(&self) -> bool {
        self.watcher
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }

    /// 测试用：直接注入一个已建好的索引（不经过扫描/落盘）。
    #[cfg(test)]
    pub fn install_cache_for_test(&self, cache: SearchCache, config: IndexConfig) {
        if let Ok(mut guard) = self.cache.lock() {
            *guard = Some(cache);
        }
        if let Ok(mut guard) = self.loaded_config.lock() {
            *guard = Some(config);
        }
    }

    /// 确保索引就绪：优先读持久化缓存，失败则后台重建。
    pub fn ensure_ready(self: &Arc<Self>, app: &AppHandle, config: IndexConfig) {
        if let Ok(mut guard) = self.config.lock() {
            *guard = config.clone();
        }

        if identity::trace_enabled() {
            eprintln!(
                "{LOG_PREFIX} 索引配置: root={:?} ignores={:?} includes={:?}",
                config.watch_root, config.ignore_paths, config.include_paths
            );
        }

        // 1) 配置未变且索引已在内存里 → 直接复用
        let unchanged = self
            .loaded_config
            .lock()
            .map(|guard| guard.as_ref() == Some(&config))
            .unwrap_or(false);
        if unchanged && self.is_ready() {
            let count = self.total_files();
            emit_status_bar(app, count);
            emit_ready(app, count);
            lifecycle::update_app_state(app, AppLifecycleState::Ready);
            // 上次监控可能因降级/异常没起来；这里补一次（幂等）
            if !self.has_watcher() {
                self.restart_watcher(Some(app), &config);
            }
            return;
        }

        // 2) 已有构建在跑 → 不重复启动
        if self.is_building() {
            eprintln!("[miaosou] 索引正在构建中，忽略本次配置更新");
            return;
        }

        // 3) 尝试读持久化缓存（配置必须完全一致，否则走重建）
        match SearchCache::try_read_persistent_cache(
            &config.watch_root,
            &self.db_path,
            &config.ignore_paths,
            &config.include_paths,
            &lifecycle::APP_QUIT,
        ) {
            Ok(cache) => {
                let count = cache.get_total_files();
                let watcher_config = config.clone();
                if let Ok(mut guard) = self.cache.lock() {
                    *guard = Some(cache);
                }
                if let Ok(mut guard) = self.loaded_config.lock() {
                    *guard = Some(config);
                }
                eprintln!(
                    "{LOG_PREFIX} 已加载持久化索引：{count} 项（{:?}）",
                    self.db_path
                );
                emit_status_bar(app, count);
                emit_ready(app, count);
                lifecycle::update_app_state(app, AppLifecycleState::Ready);
                self.restart_watcher(Some(app), &watcher_config);
            }
            Err(err) => {
                eprintln!("[miaosou] 无可用持久化索引（{err}），开始全量扫描");
                self.rebuild(app, config);
            }
        }
    }

    /// 强制重建（用户点「重建索引」/配置变更时）。
    pub fn force_rebuild(self: &Arc<Self>, app: &AppHandle) {
        let config = self.config();
        self.rebuild(app, config);
    }

    fn rebuild(self: &Arc<Self>, app: &AppHandle, config: IndexConfig) {
        if self.building.swap(true, Ordering::SeqCst) {
            eprintln!("[miaosou] 已有索引构建在运行，忽略本次重建请求");
            return;
        }

        let app = app.clone();
        let handle = self.clone();
        let spawned = std::thread::Builder::new()
            .name("miaosou-index-build".to_string())
            .spawn(move || {
                let started = Instant::now();
                lifecycle::update_app_state(&app, AppLifecycleState::Updating);

                // 扫描使用 fswalk：同一份 WalkData 既给 walker，也给进度上报线程。
                let walk_data = WalkData::new(
                    &config.watch_root,
                    &config.ignore_paths,
                    &config.include_paths,
                    false,
                    || lifecycle::APP_QUIT.load(Ordering::Relaxed),
                );
                let walking_done = AtomicBool::new(false);

                let cache = std::thread::scope(|scope| {
                    let progress_app = app.clone();
                    let progress_done = &walking_done;
                    let progress_walk_data = &walk_data;
                    scope.spawn(move || {
                        while !progress_done.load(Ordering::Relaxed) {
                            let scanned = progress_walk_data.num_dirs.load(Ordering::Relaxed)
                                + progress_walk_data.num_files.load(Ordering::Relaxed);
                            emit_status_bar(&progress_app, scanned);
                            std::thread::sleep(STATUS_EMIT_INTERVAL);
                        }
                    });

                    let cache =
                        SearchCache::walk_fs_with_walk_data(&walk_data, &lifecycle::APP_QUIT);
                    // 必须在 scope 内设置：scope 退出时会 join 进度线程，
                    // 若在 scope 外置位会导致进度线程永远看不到结束标志（死锁）。
                    walking_done.store(true, Ordering::Relaxed);
                    cache
                });

                match cache {
                    Some(cache) => {
                        let count = cache.get_total_files();
                        if let Ok(mut guard) = handle.cache.lock() {
                            *guard = Some(cache);
                        }
                        if let Ok(mut guard) = handle.loaded_config.lock() {
                            *guard = Some(config.clone());
                        }
                        eprintln!(
                            "[miaosou] 索引构建完成：{count} 项，用时 {:.1}s",
                            started.elapsed().as_secs_f64()
                        );
                        emit_status_bar(&app, count);
                        emit_ready(&app, count);
                        lifecycle::update_app_state(&app, AppLifecycleState::Ready);
                        handle.restart_watcher(Some(&app), &config);

                        // 验收自检：MIAOSOU_SELFTEST=<query> 时自动跑一次搜索并打印命中路径，
                        // 便于在目标机（UOS）无需人工输入即可验证「扫描 → 建索引 → 检索」链路。
                        if let Some(query) = identity::selftest_query() {
                            if !query.trim().is_empty() {
                                run_selftest(&handle, &query);
                            }
                        }
                    }
                    None => {
                        eprintln!("[miaosou] 索引构建被取消（退出或重新配置）");
                        lifecycle::update_app_state(&app, AppLifecycleState::Initializing);
                    }
                }

                handle.building.store(false, Ordering::SeqCst);
            });

        if spawned.is_err() {
            self.building.store(false, Ordering::SeqCst);
            eprintln!("[miaosou] 无法启动索引构建线程");
        }
    }

    pub fn search(
        &self,
        directory_query: Option<String>,
        query: Option<String>,
        case_insensitive: bool,
    ) -> Result<SearchOutcome, String> {
        let mut guard = self.cache.lock().map_err(|_| "索引锁中毒".to_string())?;
        let Some(cache) = guard.as_mut() else {
            // 索引尚未就绪：返回空结果而不是报错，前端会显示“索引更新中”
            return Ok(SearchOutcome {
                nodes: Some(Vec::new()),
                highlights: Vec::new(),
            });
        };

        cache
            .search_query_with_options(
                SearchQuery {
                    directory_query,
                    query,
                },
                SearchOptions { case_insensitive },
                CancellationToken::new_search(),
            )
            .map_err(|err| format!("搜索失败: {err}"))
    }

    pub fn expand_nodes(&self, indices: &[SlabIndex]) -> Vec<SearchResultNode> {
        if indices.is_empty() {
            return Vec::new();
        }
        match self.cache.lock() {
            Ok(mut guard) => guard
                .as_mut()
                .map(|cache| cache.expand_file_nodes(indices))
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    /// 退出时落盘（消费掉内存中的索引）。会先停掉监控线程，避免落盘期间还有事件进来。
    pub fn flush(&self) -> Result<(), String> {
        self.shutdown_watcher();
        let taken = self
            .cache
            .lock()
            .map_err(|_| "索引锁中毒".to_string())?
            .take();
        match taken {
            Some(cache) => {
                let started = Instant::now();
                cache
                    .flush_to_file(&self.db_path)
                    .map_err(|err| format!("写入索引失败: {err}"))?;
                eprintln!(
                    "[miaosou] 索引已保存到 {:?}（{:.1}s）",
                    self.db_path,
                    started.elapsed().as_secs_f64()
                );
                Ok(())
            }
            None => Ok(()),
        }
    }

    /// 用于就地查询：已索引的条目数（诊断用）。
    pub fn debug_summary(&self) -> String {
        format!(
            "ready={} building={} watching={} total={} root={:?} db={:?}",
            self.is_ready(),
            self.is_building(),
            self.has_watcher(),
            self.total_files(),
            self.config().watch_root,
            self.db_path
        )
    }
}

/// 验收自检：跑一次真实查询并打印命中路径（仅供 MIAOSOU_SELFTEST 使用）。
fn run_selftest(handle: &Arc<IndexHandle>, query: &str) {
    match handle.search(None, Some(query.to_string()), false) {
        Ok(outcome) => {
            let nodes = outcome.nodes.unwrap_or_default();
            let paths = handle.expand_nodes(&nodes);
            eprintln!("[miaosou] 自检查询 {query:?}：{} 条命中", paths.len());
            for node in paths.iter().take(10) {
                eprintln!("[miaosou]   命中：{}", node.path.to_string_lossy());
            }
        }
        Err(err) => eprintln!("[miaosou] 自检查询失败：{err}"),
    }
}

fn emit_status_bar(app: &AppHandle, scanned_files: usize) {
    let _ = app.emit_all(
        EVENT_STATUS_BAR,
        serde_json::json!({
            "scannedFiles": scanned_files,
            "processedEvents": 0,
            "rescanErrors": 0,
        }),
    );
}

fn emit_ready(app: &AppHandle, doc_count: usize) {
    let _ = app.emit_all(
        EVENT_INDEX_READY,
        serde_json::json!({ "docCount": doc_count }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "miaosou-index-test-{tag}-{}-{}",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn walks_and_searches_real_files() {
        let root = temp_root("walk");
        std::fs::write(root.join("annual-report.pdf"), b"pdf").unwrap();
        std::fs::write(root.join("notes.txt"), b"txt").unwrap();
        std::fs::create_dir_all(root.join("draft")).unwrap();
        std::fs::write(root.join("draft/report-draft.md"), b"md").unwrap();

        let cache = SearchCache::walk_fs_with_ignore(&root, &[]);
        // slab 里除了根下的条目，还包含根路径的祖先节点，因此这里只断言下界
        assert!(
            cache.get_total_files() >= 4,
            "至少包含 3 个文件 + 1 个目录，实际 {}",
            cache.get_total_files()
        );

        // 子串搜索
        let mut cache = cache;
        let outcome = cache
            .search_query_with_options(
                SearchQuery {
                    directory_query: None,
                    query: Some("report".to_string()),
                },
                SearchOptions::default(),
                CancellationToken::noop(),
            )
            .unwrap();
        let nodes = outcome.nodes.unwrap_or_default();
        assert_eq!(nodes.len(), 2, "annual-report.pdf 与 report-draft.md");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn incremental_event_adds_and_removes_file() {
        use cardinal_sdk::{EventFlag, FsEvent};

        let root = temp_root("incremental");
        std::fs::write(root.join("existing.txt"), b"x").unwrap();

        let config = IndexConfig::new(root.clone(), Vec::new(), Vec::new());
        let index = IndexHandle::new(config.clone(), root.join("index.db"));
        index.install_cache_for_test(SearchCache::walk_fs_with_ignore(&root, &[]), config.clone());

        // 模拟 watcher 送来「新文件」事件（根目录事件由 watcher 侧丢弃，见 watcher.rs）
        let nodes_before = index.total_files();
        let new_file = root.join("brand-new-report.pdf");
        std::fs::write(&new_file, b"x").unwrap();
        index
            .apply_fs_events(
                vec![FsEvent {
                    path: new_file.clone(),
                    flag: EventFlag::ItemModified | EventFlag::ItemIsFile,
                    id: 2,
                }],
                None,
            )
            .expect("应用事件应成功");

        // 注意：不能用绝对节点数断言——不同平台的根路径祖先链长度不同
        // （macOS 的 /private/var/... 比 Linux 的 /tmp 深，节点数会差几个）
        assert_eq!(
            index.total_files(),
            nodes_before + 1,
            "增量更新后应恰好新增 1 个节点"
        );
        let hits = index
            .search(None, Some("brand-new".to_string()), false)
            .expect("搜索应成功")
            .nodes
            .unwrap_or_default();
        assert_eq!(hits.len(), 1, "新增文件应能被搜索到");

        // 删除事件同样要生效
        std::fs::remove_file(&new_file).unwrap();
        index
            .apply_fs_events(
                vec![FsEvent {
                    path: new_file,
                    flag: EventFlag::ItemModified,
                    id: 3,
                }],
                None,
            )
            .expect("应用删除事件应成功");
        let hits = index
            .search(None, Some("brand-new".to_string()), false)
            .expect("搜索应成功")
            .nodes
            .unwrap_or_default();
        assert!(hits.is_empty(), "删除的文件应从索引中移除");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn persists_and_reloads_cache() {
        let root = temp_root("persist");
        std::fs::write(root.join("report-final.pdf"), b"pdf").unwrap();
        std::fs::write(root.join("notes.txt"), b"txt").unwrap();
        let db = root.join("cache.db");

        // 建索引 → 落盘（flush_to_file 会消费 cache）
        let cache = SearchCache::walk_fs_with_ignore(&root, &[]);
        cache.flush_to_file(&db).expect("索引应能写入磁盘");
        assert!(db.exists(), "落盘后应存在索引文件");

        // 重新加载 → 搜索仍然命中
        let mut reloaded = SearchCache::try_read_persistent_cache(
            &root,
            &db,
            &Vec::new(),
            &Vec::new(),
            &lifecycle::APP_QUIT,
        )
        .expect("应能读回持久化索引");
        let outcome = reloaded
            .search_query_with_options(
                SearchQuery {
                    directory_query: None,
                    query: Some("report".to_string()),
                },
                SearchOptions::default(),
                CancellationToken::noop(),
            )
            .unwrap();
        assert_eq!(outcome.nodes.unwrap_or_default().len(), 1);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn default_ignore_paths_cover_linux_noise() {
        let paths = default_ignore_paths();
        let rendered: Vec<String> = paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        assert!(rendered.iter().any(|path| path == "/proc"));
        assert!(rendered.iter().any(|path| path == "/sys"));
        assert!(rendered.iter().any(|path| path.ends_with("/.cache")));
    }

    /// 性能基准（手动）：合成大目录树，测全量扫描、查询延迟、内存峰值与索引体积。
    ///
    /// 运行（目标机 / Debian 10 容器）：
    ///   cargo test --release --features custom-protocol perf_benchmark -- --ignored --nocapture
    ///
    /// 环境变量：
    ///   MIAOSOU_BENCH_FILES     文件数（默认 200000）
    ///   MIAOSOU_BENCH_DIRS      目录数（默认 500）
    ///   MIAOSOU_BENCH_QUERIES   查询次数（默认 200）
    ///   MIAOSOU_BENCH_KEEP=1    保留合成目录（默认清理）
    #[test]
    #[ignore = "性能基准：合成大目录树，手动运行"]
    fn perf_benchmark_synthetic_tree() {
        use std::time::Instant;

        fn env_usize(key: &str, default: usize) -> usize {
            std::env::var(key)
                .ok()
                .and_then(|raw| raw.parse::<usize>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(default)
        }

        fn percentile(sorted_micros: &[u128], percent: usize) -> u128 {
            if sorted_micros.is_empty() {
                return 0;
            }
            let index = (sorted_micros.len() - 1) * percent / 100;
            sorted_micros[index]
        }

        /// 读 /proc/self/status 的内存字段（KB）；非 Linux 返回 None。
        fn proc_status_kb(key: &str) -> Option<u64> {
            let text = std::fs::read_to_string("/proc/self/status").ok()?;
            text.lines().find_map(|line| {
                line.strip_prefix(key)?
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()
            })
        }

        let files = env_usize("MIAOSOU_BENCH_FILES", 200_000);
        let dirs = env_usize("MIAOSOU_BENCH_DIRS", 500);
        let query_count = env_usize("MIAOSOU_BENCH_QUERIES", 200);
        let keep = std::env::var_os("MIAOSOU_BENCH_KEEP").is_some();
        let per_dir = (files / dirs).max(1);

        let root = temp_root("perf");
        let started = Instant::now();
        let mut names: Vec<String> = Vec::with_capacity(per_dir * dirs);
        for dir_index in 0..dirs {
            let dir = root.join(format!("dir{dir_index:04}"));
            std::fs::create_dir_all(&dir).unwrap();
            for file_index in 0..per_dir {
                let name = format!("report_{dir_index:04}_{file_index:05}.txt");
                std::fs::write(dir.join(&name), b"benchmark-payload").unwrap();
                names.push(name);
            }
        }
        let generated = dirs * per_dir;
        println!(
            "BENCH generate: files={generated} dirs={dirs} elapsed_ms={}",
            started.elapsed().as_millis()
        );
        println!(
            "BENCH memory_before_index: rss_kb={:?} peak_kb={:?}",
            proc_status_kb("VmRSS:"),
            proc_status_kb("VmHWM:")
        );

        // 全量扫描 + 建索引（与 App 一致：need_metadata=false）
        let started = Instant::now();
        let mut cache = SearchCache::walk_fs_with_ignore(&root, &[]);
        let walk_elapsed = started.elapsed();
        let nodes = cache.get_total_files();
        let files_per_sec = generated as f64 / walk_elapsed.as_secs_f64().max(1e-9);
        println!(
            "BENCH walk: elapsed_ms={} files={generated} nodes={nodes} files_per_sec={files_per_sec:.0}",
            walk_elapsed.as_millis()
        );
        println!(
            "BENCH memory: rss_kb={:?} peak_kb={:?}",
            proc_status_kb("VmRSS:"),
            proc_status_kb("VmHWM:")
        );

        // 查询延迟（确定性伪随机，避免引额外依赖）
        let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next_index = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 33) as usize) % names.len()
        };
        let mut durations: Vec<u128> = Vec::with_capacity(query_count);
        let mut total_hits = 0usize;
        for _ in 0..query_count {
            let name = names[next_index()].clone();
            let started = Instant::now();
            let outcome = cache
                .search_query_with_options(
                    SearchQuery {
                        directory_query: None,
                        query: Some(name),
                    },
                    SearchOptions::default(),
                    CancellationToken::noop(),
                )
                .expect("查询应成功");
            durations.push(started.elapsed().as_micros());
            total_hits += outcome.nodes.unwrap_or_default().len();
        }
        durations.sort_unstable();
        println!(
            "BENCH query: count={query_count} hits={total_hits} p50_us={} p95_us={} max_us={}",
            percentile(&durations, 50),
            percentile(&durations, 95),
            durations.last().copied().unwrap_or(0)
        );

        // 持久化：写盘耗时与索引体积
        let db_path = root.join("perf-index.db");
        let started = Instant::now();
        cache.flush_to_file(&db_path).expect("索引应能落盘");
        let flush_elapsed = started.elapsed();
        let index_bytes = std::fs::metadata(&db_path)
            .map(|meta| meta.len())
            .unwrap_or(0);
        println!(
            "BENCH persist: flush_ms={} index_bytes={index_bytes} index_mb={:.1}",
            flush_elapsed.as_millis(),
            index_bytes as f64 / 1024.0 / 1024.0
        );

        let _ = std::fs::remove_file(&db_path);
        if keep {
            println!("BENCH keep: {}", root.display());
        } else {
            std::fs::remove_dir_all(&root).ok();
        }
    }
}
