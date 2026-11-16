use crate::{
    FileNodes, NameIndex, SearchOptions, SearchResultNode, SlabIndex, SlabNode,
    SlabNodeMetadataCompact, State, ThinSlab,
    highlight::derive_highlight_terms,
    persistent::{PersistentStorage, read_cache_from_file, write_cache_to_file},
    query_preprocessor::{expand_query_home_dirs, strip_query_quotes},
};
use anyhow::{Context, Result, anyhow};
use cardinal_sdk::{EventFlag, FsEvent, ScanType, current_event_id};
use cardinal_syntax::{Expr, Filter, FilterKind, Term, optimize_query, parse_query};
use fswalk::{
    Node, NodeMetadata, WalkData, should_ignore_path, walk_it, walk_it_without_root_chain,
};
use hashbrown::HashSet;
use namepool::NamePool;
use search_cancel::CancellationToken;
use std::{
    ffi::OsStr,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{
        LazyLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use thin_vec::ThinVec;
use tracing::{debug, info};
use typed_num::Num;
use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfc_quick, is_nfd_quick};

/// A flag that is never set
static NEVER_STOPPED: AtomicBool = AtomicBool::new(false);

pub struct SearchCache {
    pub(crate) file_nodes: FileNodes,
    last_event_id: u64,
    rescan_count: u64,
    pub(crate) name_index: NameIndex,
    stop: &'static AtomicBool,
}

#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    /// Directory query
    pub directory_query: Option<String>,
    /// File query
    pub query: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SearchOutcome {
    /// `None` means search was cancelled (not "no match");
    /// `Some(vec![])` means completed search with zero matches.
    pub nodes: Option<Vec<SlabIndex>>,
    pub highlights: Vec<String>,
}

impl SearchOutcome {
    fn new(nodes: Option<Vec<SlabIndex>>, highlights: Vec<String>) -> Self {
        Self { nodes, highlights }
    }

    fn cancelled() -> Self {
        Self {
            nodes: None,
            highlights: vec![],
        }
    }

    fn is_cancelled(&self) -> bool {
        self.nodes.is_none()
    }

    fn merge(self, other: Self) -> Self {
        let SearchOutcome {
            nodes: primary_nodes,
            highlights: primary_highlights,
        } = self;
        let SearchOutcome {
            nodes: secondary_nodes,
            highlights: secondary_highlights,
        } = other;

        let (Some(primary_nodes), Some(secondary_nodes)) = (primary_nodes, secondary_nodes) else {
            return Self::cancelled();
        };

        let merged_nodes = Self::merge_preserve_order(primary_nodes, secondary_nodes);
        let merged_highlights =
            Self::merge_preserve_order(primary_highlights, secondary_highlights);
        Self::new(Some(merged_nodes), merged_highlights)
    }

    fn merge_preserve_order<T>(lhs: Vec<T>, rhs: Vec<T>) -> Vec<T>
    where
        T: Eq + std::hash::Hash + Clone,
    {
        let mut merged = Vec::with_capacity(lhs.len() + rhs.len());
        let mut seen = HashSet::with_capacity(lhs.len() + rhs.len());

        for item in lhs.into_iter().chain(rhs.into_iter()) {
            if seen.insert(item.clone()) {
                merged.push(item);
            }
        }

        merged
    }
}

impl std::fmt::Debug for SearchCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchCache")
            .field("path", &self.file_nodes.path())
            .field("last_event_id", &self.last_event_id)
            .field("rescan_count", &self.rescan_count)
            .field("slab_root", &self.file_nodes.root())
            .field("slab.len()", &self.file_nodes.len())
            .field("name_index.len()", &self.name_index.len())
            .finish()
    }
}

impl SearchCache {
    pub fn ignore_paths(&self) -> Box<[PathBuf]> {
        self.file_nodes.ignore_paths().clone().into_boxed_slice()
    }

    pub fn include_paths(&self) -> Box<[PathBuf]> {
        self.file_nodes.include_paths().clone().into_boxed_slice()
    }

    /// The `path` is the root path of the constructed cache and fsevent watch path.
    pub fn try_read_persistent_cache(
        path: &Path,
        cache_path: &Path,
        current_ignore_paths: &Vec<PathBuf>,
        current_include_paths: &Vec<PathBuf>,
        cancel: &'static AtomicBool,
    ) -> Result<Self> {
        read_cache_from_file(cache_path)
            .and_then(|x| {
                (x.path == path)
                    .then_some(())
                    .ok_or_else(|| {
                        anyhow!(
                            "Inconsistent root path: expected: {:?}, actual: {:?}",
                            path,
                            &x.path
                        )
                    })
                    .map(|()| x)
            })
            .and_then(|x| {
                (&x.ignore_paths == current_ignore_paths)
                    .then_some(())
                    .ok_or_else(|| {
                        anyhow!(
                            "Inconsistent ignore paths: expected: {:?}, actual: {:?}",
                            &current_ignore_paths,
                            &x.ignore_paths
                        )
                    })
                    .map(|()| x)
            })
            .and_then(|x| {
                (&x.include_paths == current_include_paths)
                    .then_some(())
                    .ok_or_else(|| {
                        anyhow!(
                            "Inconsistent include paths: expected: {:?}, actual: {:?}",
                            &current_include_paths,
                            &x.include_paths
                        )
                    })
                    .map(|()| x)
            })
            .map(
                |PersistentStorage {
                     version: _,
                     path,
                     ignore_paths,
                     include_paths,
                     slab_root,
                     slab,
                     name_index,
                     last_event_id,
                     rescan_count,
                 }| {
                    // name pool construction speed is fast enough that caching it doesn't worth it.
                    let name_index = NameIndex::construct_name_pool(name_index);
                    let slab = FileNodes::new(path, ignore_paths, include_paths, slab, slab_root);
                    Self::new(slab, last_event_id, rescan_count, name_index, cancel)
                },
            )
    }

    /// Get the total number of files and directories in the cache.
    pub fn get_total_files(&self) -> usize {
        self.file_nodes.len()
    }

    pub fn walk_fs_with_ignore(path: &Path, ignore_paths: &[PathBuf]) -> Self {
        Self::walk_fs_with_walk_data(
            &WalkData::new(path, ignore_paths, &[], false, || false),
            &NEVER_STOPPED,
        )
        .unwrap()
    }

    pub fn walk_fs(path: &Path) -> Self {
        Self::walk_fs_with_walk_data(
            &WalkData::new(path, &[], &[], false, || false),
            &NEVER_STOPPED,
        )
        .unwrap()
    }

    /// This function is expected to be called with WalkData which metadata is not fetched.
    /// If cancelled during walking, None is returned.
    pub fn walk_fs_with_walk_data<F>(
        walk_data: &WalkData<'_, F>,
        cancel: &'static AtomicBool,
    ) -> Option<Self>
    where
        F: Fn() -> bool + Send + Sync,
    {
        // Return None if cancelled
        fn walkfs_to_slab<F>(
            walk_data: &WalkData<'_, F>,
        ) -> Option<(SlabIndex, ThinSlab<SlabNode>, NameIndex)>
        where
            F: Fn() -> bool + Send + Sync,
        {
            // Build the tree of file names in parallel first (we cannot construct the slab directly
            // because slab nodes reference each other and we prefer to avoid locking).
            let visit_time = Instant::now();
            let Some(node) = walk_it(walk_data) else {
                info!("walk filesystem cancelled during walk_it.");
                return None;
            };
            info!(
                "Walk data: {:?}, time: {:?}",
                walk_data,
                visit_time.elapsed()
            );

            // Then create the slab.
            let slab_time = Instant::now();
            let mut slab = ThinSlab::new();
            let mut name_index = NameIndex::default();
            let slab_root = construct_node_slab_name_index(None, &node, &mut slab, &mut name_index);
            info!(
                "Slab & NameIndex construction time: {:?}, slab root: {:?}, slab len: {:?}",
                slab_time.elapsed(),
                slab_root,
                slab.len()
            );

            Some((slab_root, slab, name_index))
        }

        let last_event_id = current_event_id();
        let (slab_root, slab, name_index) = walkfs_to_slab(walk_data)?;
        let slab = FileNodes::new(
            walk_data.root_path.to_path_buf(),
            walk_data.ignore_directories.to_vec(),
            walk_data.include_paths.to_vec(),
            slab,
            slab_root,
        );
        // metadata cache inits later
        Some(Self::new(slab, last_event_id, 0, name_index, cancel))
    }

    fn new(
        slab: FileNodes,
        last_event_id: u64,
        rescan_count: u64,
        name_index: NameIndex,
        cancel: &'static AtomicBool,
    ) -> Self {
        Self {
            file_nodes: slab,
            last_event_id,
            rescan_count,
            name_index,
            stop: cancel,
        }
    }

    /// Create a simple SearchCache which doesn't contain any file node and is
    /// expected to be used when walk_fs is cancelled.
    pub fn noop(
        path: PathBuf,
        ignore_paths: Vec<PathBuf>,
        include_paths: Vec<PathBuf>,
        cancel: &'static AtomicBool,
    ) -> Self {
        Self {
            file_nodes: FileNodes::new(
                path,
                ignore_paths,
                include_paths,
                ThinSlab::new(),
                SlabIndex::new(0),
            ),
            last_event_id: 0,
            rescan_count: 0,
            name_index: NameIndex::default(),
            stop: cancel,
        }
    }

    pub fn is_noop(&self) -> bool {
        self.file_nodes.is_empty() && self.name_index.is_empty()
    }

    pub fn search_empty(&self, cancellation_token: CancellationToken) -> Option<Vec<SlabIndex>> {
        self.name_index.all_indices(cancellation_token)
    }

    #[cfg(test)]
    pub fn search(&mut self, line: &str) -> Result<Vec<SlabIndex>> {
        self.search_with_options(line, SearchOptions::default(), CancellationToken::noop())
            .map(|outcome| outcome.nodes.unwrap_or_default())
    }

    pub fn search_query_with_options(
        &mut self,
        query: SearchQuery,
        options: SearchOptions,
        cancellation_token: CancellationToken,
    ) -> Result<SearchOutcome> {
        match (&query.directory_query, &query.query) {
            // Both fields are active: resolve folder scope first, then use that
            // descendant set as the base for the normal file search.
            (Some(directory_query), Some(query)) => self.search_with_directory_scope(
                directory_query,
                query,
                options,
                cancellation_token,
            ),
            // Only the main search box is active: keep the existing single-pass behavior.
            (None, Some(query)) => self.search_with_options(query, options, cancellation_token),
            // Only the folder scope box is active: run a single-pass folder-only search.
            (Some(directory_query), None) => {
                self.search_folder_with_options(directory_query, options, cancellation_token)
            }
            // No user input: return the existing empty-query result set directly.
            (None, None) => Ok(self
                .search_empty(cancellation_token)
                .map(|nodes| SearchOutcome::new(Some(nodes), vec![]))
                .unwrap_or_else(SearchOutcome::cancelled)),
        }
    }

    pub fn search_with_options(
        &mut self,
        line: &str,
        options: SearchOptions,
        cancellation_token: CancellationToken,
    ) -> Result<SearchOutcome> {
        self.search_with_options_base(line, None, options, cancellation_token)
    }

    fn search_folder_with_options(
        &mut self,
        line: &str,
        options: SearchOptions,
        cancellation_token: CancellationToken,
    ) -> Result<SearchOutcome> {
        self.search_with_expr_transform(
            line,
            None,
            options,
            cancellation_token,
            require_folder_expr,
        )
    }

    fn search_with_options_base(
        &mut self,
        line: &str,
        base: Option<&Vec<SlabIndex>>,
        options: SearchOptions,
        cancellation_token: CancellationToken,
    ) -> Result<SearchOutcome> {
        self.search_with_expr_transform(line, base, options, cancellation_token, |expr| expr)
    }

    fn search_with_expr_transform(
        &mut self,
        line: &str,
        base: Option<&Vec<SlabIndex>>,
        options: SearchOptions,
        cancellation_token: CancellationToken,
        transform: impl Fn(Expr) -> Expr + Copy,
    ) -> Result<SearchOutcome> {
        let primary = self.search_with_query_line_transform_base(
            line,
            base,
            options,
            cancellation_token,
            transform,
        )?;
        // Cancellation must short-circuit and propagate as `nodes: None`.
        // Running a secondary normalization pass after cancellation would be wasted work.
        if primary.is_cancelled() {
            return Ok(SearchOutcome::cancelled());
        }

        // Best-effort APFS workaround: run primary query first, then optionally
        // run one alternate normalization form when the input is normalization-sensitive.
        let Some(alt_line) = self.alternate_normalization_query(line) else {
            return Ok(primary);
        };

        let secondary = self.search_with_query_line_transform_base(
            &alt_line,
            base,
            options,
            cancellation_token,
            transform,
        )?;
        Ok(primary.merge(secondary))
    }

    fn search_directory_scope(
        &mut self,
        directory_query: &str,
        options: SearchOptions,
        cancellation_token: CancellationToken,
    ) -> Result<SearchOutcome> {
        let outcome =
            self.search_folder_with_options(directory_query, options, cancellation_token)?;
        if outcome.is_cancelled() {
            return Ok(SearchOutcome::cancelled());
        }

        let SearchOutcome { nodes, highlights } = outcome;
        let Some(nodes) = nodes else {
            return Ok(SearchOutcome::cancelled());
        };

        let mut scoped_nodes = Vec::new();
        for (i, index) in nodes.into_iter().enumerate() {
            if cancellation_token.is_cancelled_sparse(i).is_none() {
                return Ok(SearchOutcome::cancelled());
            }
            let Some(descendants) = self.all_subnodes(index, cancellation_token) else {
                return Ok(SearchOutcome::cancelled());
            };
            scoped_nodes.extend(descendants);
        }

        let scoped_nodes = SearchOutcome::merge_preserve_order(Vec::new(), scoped_nodes);
        Ok(SearchOutcome::new(Some(scoped_nodes), highlights))
    }

    fn search_with_directory_scope(
        &mut self,
        directory_query: &str,
        query: &str,
        options: SearchOptions,
        cancellation_token: CancellationToken,
    ) -> Result<SearchOutcome> {
        let scope = self.search_directory_scope(directory_query, options, cancellation_token)?;
        let SearchOutcome {
            nodes: scope_nodes,
            highlights: scope_highlights,
        } = scope;
        let Some(scope_nodes) = scope_nodes else {
            return Ok(SearchOutcome::cancelled());
        };

        let primary =
            self.search_with_options_base(query, Some(&scope_nodes), options, cancellation_token)?;
        let SearchOutcome {
            nodes: primary_nodes,
            highlights: primary_highlights,
        } = primary;
        let Some(primary_nodes) = primary_nodes else {
            return Ok(SearchOutcome::cancelled());
        };

        let highlights = SearchOutcome::merge_preserve_order(scope_highlights, primary_highlights);
        Ok(SearchOutcome::new(Some(primary_nodes), highlights))
    }

    fn search_with_query_line_transform_base(
        &mut self,
        line: &str,
        base: Option<&Vec<SlabIndex>>,
        options: SearchOptions,
        cancellation_token: CancellationToken,
        transform: impl Fn(Expr) -> Expr,
    ) -> Result<SearchOutcome> {
        let parsed = parse_query(line).map_err(|err| anyhow!("Failed to parse query: {err}"))?;
        let expanded = expand_query_home_dirs(parsed);
        let mut unquoted = strip_query_quotes(expanded);
        let highlights = derive_highlight_terms(&unquoted.expr);
        unquoted.expr = transform(unquoted.expr);
        let optimized = optimize_query(unquoted);
        let search_time = Instant::now();
        let result = self.evaluate_expr(&optimized.expr, base, options, cancellation_token);
        info!("Search time: {:?}", search_time.elapsed());
        result.map(|nodes| SearchOutcome::new(nodes, highlights))
    }

    // Why this exists:
    // - APFS may surface path segments in either NFC or NFD (unlike HFS+ forcing NFD).
    // - Our matcher is byte-oriented, so canonically equivalent Unicode forms
    //   can miss each other without an alternate query form.
    //
    // Performance:
    // - `is_nfd_quick` / `is_nfc_quick` are cheap probes.
    // - If both are `Yes`, query is normalization-inert, so we skip the second pass.
    //
    // Limitations (by design):
    // - This is a low-overhead workaround, not a fully normalization-aware index.
    // - We only generate one alternate query line.
    // - A filename that mixes NFC/NFD within the same segment can still be a miss,
    //   depending on which form the matcher ultimately compares against.
    fn alternate_normalization_query(&self, line: &str) -> Option<String> {
        let nfd_quick = is_nfd_quick(line.chars());
        let nfc_quick = is_nfc_quick(line.chars());
        let nfd_yes = matches!(nfd_quick, IsNormalized::Yes);
        let nfc_yes = matches!(nfc_quick, IsNormalized::Yes);
        if nfd_yes && nfc_yes {
            return None;
        }

        let alt = if nfd_yes {
            line.nfc().collect::<String>()
        } else {
            line.nfd().collect::<String>()
        };
        (alt != line).then_some(alt)
    }

    /// Get the path of the node in the slab.
    pub fn node_path(&self, index: SlabIndex) -> Option<PathBuf> {
        self.file_nodes.node_path(index)
    }

    /// Locate the slab index for an absolute path when it belongs to the watch root.
    pub fn node_index_for_path(&self, path: &Path) -> Option<SlabIndex> {
        self.node_index_for_path_with_case(path, false)
    }

    pub(crate) fn node_index_for_path_with_case(
        &self,
        path: &Path,
        case_insensitive: bool,
    ) -> Option<SlabIndex> {
        let Ok(path) = path.strip_prefix("/") else {
            return None;
        };
        let mut current = self.file_nodes.root();
        for segment in path {
            let next = self.file_nodes[current]
                .children
                .iter()
                .find_map(|&child| {
                    let name = self.file_nodes[child].name();
                    path_segment_matches(name, segment, case_insensitive).then_some(child)
                })?;
            current = next;
        }
        Some(current)
    }

    /// Get all subnode indices of a given node index
    pub fn all_subnodes(
        &self,
        index: SlabIndex,
        cancel: CancellationToken,
    ) -> Option<Vec<SlabIndex>> {
        let mut result = Vec::new();
        let mut i = 0;
        self.all_subnodes_recursive(index, &mut result, &mut i, cancel)?;
        Some(result)
    }

    fn all_subnodes_recursive(
        &self,
        index: SlabIndex,
        out: &mut Vec<SlabIndex>,
        i: &mut usize,
        cancel: CancellationToken,
    ) -> Option<()> {
        for &child in &self.file_nodes[index].children {
            cancel.is_cancelled_sparse(*i)?;
            *i += 1;
            out.push(child);
            self.all_subnodes_recursive(child, out, i, cancel)?;
        }
        Some(())
    }

    fn push_node(&mut self, node: SlabNode) -> SlabIndex {
        let name = node.name();
        let index = self.file_nodes.insert(node);
        self.name_index.add_index(name, index, &self.file_nodes);
        index
    }

    /// Removes a node by path and its children recursively.
    fn remove_node_path(&mut self, path: &Path) -> Option<SlabIndex> {
        let Ok(path) = path.strip_prefix("/") else {
            return None;
        };
        let mut current = self.file_nodes.root();
        for name in path {
            if let Some(&index) = self.file_nodes[current]
                .children
                .iter()
                .find(|&&x| self.file_nodes[x].name() == name)
            {
                current = index;
            } else {
                return None;
            }
        }
        self.remove_node(current);
        Some(current)
    }

    // Create node chain of specific path
    fn create_node_chain(&mut self, path: &Path) -> SlabIndex {
        let path = path
            .strip_prefix("/")
            .expect("create_node_chain only accepts absolute path");
        let mut current = self.file_nodes.root();
        let mut current_path = PathBuf::from("/");
        for name in path {
            current_path.push(name);
            current = if let Some(&index) = self.file_nodes[current]
                .children
                .iter()
                .find(|&&x| self.file_nodes[x].name() == name)
            {
                index
            } else {
                let metadata = std::fs::symlink_metadata(&current_path)
                    .map(NodeMetadata::from)
                    .ok();
                let name = NAME_POOL.push(name.to_string_lossy().as_ref());
                let node = SlabNode::new(
                    Some(current),
                    name,
                    match metadata {
                        Some(metadata) => SlabNodeMetadataCompact::some(metadata),
                        None => SlabNodeMetadataCompact::unaccessible(),
                    },
                );
                let index = self.push_node(node);
                self.file_nodes[current].add_children(index);
                index
            };
        }
        current
    }

    fn should_ignore(&self, path: &Path) -> bool {
        should_ignore_path(
            path,
            self.file_nodes.ignore_paths(),
            self.file_nodes.include_paths(),
        )
    }

    // `Self::scan_path_recursive`function returns index of the constructed node(with metadata provided).
    // - If path is not under the watch root, None is returned.
    // - Procedure contains metadata fetching, if metadata fetching failed, None is returned.
    fn scan_path_recursive(&mut self, path: &Path) -> Option<SlabIndex> {
        // Ensure path is under the watch root
        if path.symlink_metadata().err().map(|e| e.kind()) == Some(ErrorKind::NotFound) {
            self.remove_node_path(path);
            return None;
        };
        if self.should_ignore(path) {
            return None;
        }
        let parent = path.parent().expect(
            "scan_path_recursive doesn't expected to scan root(should be filtered outside)",
        );
        // Ensure node of the path parent is existed
        let parent = self.create_node_chain(parent);
        // Remove node(if exists) and do a full rescan
        if let Some(&old_node) = self.file_nodes[parent]
            .children
            .iter()
            .find(|&&x| path.file_name() == Some(OsStr::new(self.file_nodes[x].name())))
        {
            self.remove_node(old_node);
        }
        // For incremental data, we need metadata
        let walk_data = WalkData::new(
            path,
            self.file_nodes.ignore_paths(),
            self.file_nodes.include_paths(),
            true,
            || self.stop.load(Ordering::Relaxed),
        );
        walk_it_without_root_chain(&walk_data).map(|node| {
            let node = self.create_node_slab_update_name_index_and_name_pool(Some(parent), &node);
            // Push the newly created node to the parent's children
            self.file_nodes[parent].add_children(node);
            node
        })
    }

    // `Self::scan_path_nonrecursive`function returns index of the constructed node.
    // - If path is not under the watch root, None is returned.
    // - Procedure contains metadata fetching, if metadata fetching failed, None is returned.
    #[allow(dead_code)]
    fn scan_path_nonrecursive(&mut self, path: &Path) -> Option<SlabIndex> {
        // Ensure path is under the watch root
        if path.symlink_metadata().err().map(|e| e.kind()) == Some(ErrorKind::NotFound) {
            self.remove_node_path(path);
            return None;
        };
        Some(self.create_node_chain(path))
    }

    pub fn walk_data<'p>(
        &self,
        phantom1: &'p mut PathBuf,
        phantom2: &'p mut Vec<PathBuf>,
        phantom3: &'p mut Vec<PathBuf>,
        scan_cancellation_token: CancellationToken,
    ) -> WalkData<'p, impl Fn() -> bool + Send + Sync + Copy + 'static> {
        *phantom1 = self.file_nodes.path().to_path_buf();
        *phantom2 = self.file_nodes.ignore_paths().clone();
        *phantom3 = self.file_nodes.include_paths().clone();
        let stop = self.stop;
        WalkData::new(phantom1, phantom2, phantom3, false, move || {
            stop.load(Ordering::Relaxed) || scan_cancellation_token.is_cancelled().is_none()
        })
    }

    pub fn rescan_with_walk_data<F>(&mut self, walk_data: &WalkData<'_, F>) -> Option<()>
    where
        F: Fn() -> bool + Send + Sync,
    {
        let Some(new_cache) = Self::walk_fs_with_walk_data(walk_data, self.stop) else {
            info!("Rescan cancelled.");
            return None;
        };
        *self = new_cache;
        Some(())
    }

    pub fn rescan(&mut self) {
        // Remove all memory consuming cache early for memory consumption in Self::walk_fs_new.
        let Some(new_cache) = Self::walk_fs_with_walk_data(
            &WalkData::new(
                self.file_nodes.path(),
                self.file_nodes.ignore_paths(),
                self.file_nodes.include_paths(),
                false,
                || self.stop.load(Ordering::Relaxed),
            ),
            self.stop,
        ) else {
            info!("Rescan cancelled.");
            return;
        };
        *self = new_cache;
    }

    /// Removes a node and its children recursively by index.
    fn remove_node(&mut self, index: SlabIndex) {
        fn remove_single_node(cache: &mut SearchCache, index: SlabIndex) {
            if let Some(node) = cache.file_nodes.try_remove(index) {
                let removed = cache.name_index.remove_index(node.name(), index);
                assert!(removed, "inconsistent name index and node");
            }
        }

        // Remove parent reference, make whole subtree unreachable.
        if let Some(parent) = self.file_nodes[index].parent() {
            self.file_nodes[parent].children.retain(|&x| x != index);
        }
        let mut stack = vec![index];
        while let Some(current) = stack.pop() {
            stack.extend_from_slice(&self.file_nodes[current].children);
            remove_single_node(self, current);
        }
    }

    pub fn flush_snapshot_to_file(&mut self, cache_path: &Path) -> Result<()> {
        let name_index = self.name_index.as_persistent();
        let slab = self.file_nodes.take_slab();

        let storage = PersistentStorage {
            version: Num,
            last_event_id: self.last_event_id,
            rescan_count: self.rescan_count,
            path: self.file_nodes.path().to_path_buf(),
            ignore_paths: self.file_nodes.ignore_paths().clone(),
            include_paths: self.file_nodes.include_paths().clone(),
            slab_root: self.file_nodes.root(),
            name_index,
            slab,
        };

        let flush_result =
            write_cache_to_file(cache_path, &storage).context("Write cache to file failed.");

        let PersistentStorage { slab, .. } = storage;
        self.file_nodes.put_slab(slab);

        flush_result
    }

    pub fn flush_to_file(self, cache_path: &Path) -> Result<()> {
        let Self {
            file_nodes,
            last_event_id,
            rescan_count,
            name_index,
            stop: _,
        } = self;
        let (path, ignore_paths, include_paths, slab_root, slab) = file_nodes.into_parts();
        let name_index = name_index.into_persistent();
        write_cache_to_file(
            cache_path,
            &PersistentStorage {
                version: Num,
                path,
                ignore_paths,
                include_paths,
                slab_root,
                slab,
                name_index,
                last_event_id,
                rescan_count,
            },
        )
        .context("Write cache to file failed.")
    }

    fn update_last_event_id(&mut self, event_id: u64) {
        if event_id <= self.last_event_id {
            debug!("last_event_id {} |< {event_id}", self.last_event_id);
        } else {
            debug!("last_event_id {} => {event_id}", self.last_event_id);
            self.last_event_id = event_id;
        }
    }

    pub fn last_event_id(&mut self) -> u64 {
        self.last_event_id
    }

    pub fn rescan_count(&self) -> u64 {
        self.rescan_count
    }

    /// Note that this function doesn't fetch metadata (even if it's not cached) for the nodes.
    pub fn query_files(
        &mut self,
        query: &str,
        cancellation_token: CancellationToken,
    ) -> Result<Option<Vec<SearchResultNode>>> {
        self.search_with_options(query, SearchOptions::default(), cancellation_token)
            .map(|outcome| {
                outcome
                    .nodes
                    .map(|nodes| self.expand_file_nodes_inner::<false>(&nodes))
            })
    }

    /// Returns a node info vector with the same length as the input nodes.
    /// If the given node is not found, an empty SearchResultNode is returned.
    pub fn expand_file_nodes(&mut self, nodes: &[SlabIndex]) -> Vec<SearchResultNode> {
        self.expand_file_nodes_inner::<true>(nodes)
    }

    fn expand_file_nodes_inner<const FETCH_META: bool>(
        &mut self,
        nodes: &[SlabIndex],
    ) -> Vec<SearchResultNode> {
        nodes
            .iter()
            .copied()
            .map(|node_index| {
                let path = self.node_path(node_index);
                let metadata = self
                    .file_nodes
                    .get_mut(node_index)
                    .map(|node| {
                        match (node.state(), &path) {
                            (State::None, Some(path)) if FETCH_META => {
                                // try fetching metadata if it's not cached and cache them
                                let metadata = match std::fs::symlink_metadata(path) {
                                    Ok(metadata) => SlabNodeMetadataCompact::some(metadata.into()),
                                    Err(_) => SlabNodeMetadataCompact::unaccessible(),
                                };
                                node.metadata = metadata;
                                metadata
                            }
                            _ => node.metadata,
                        }
                    })
                    .unwrap_or_else(SlabNodeMetadataCompact::unaccessible);
                SearchResultNode {
                    path: path.unwrap_or_default(),
                    metadata,
                }
            })
            .collect()
    }

    pub fn handle_fs_events(&mut self, events: Vec<FsEvent>) -> Result<(), HandleFSEError> {
        let max_event_id = events.iter().map(|e| e.id).max();
        // If rescan needed, early exit.
        if events.iter().any(|event| {
            if event.flag.contains(EventFlag::HistoryDone) {
                info!("History processing done: {:?}", event);
            }
            if event.should_rescan(self.file_nodes.path()) {
                info!("Event rescan: {:?}", event);
                true
            } else {
                false
            }
        }) {
            self.rescan_count = self.rescan_count.saturating_add(1);
            return Err(HandleFSEError::Rescan);
        }
        for scan_path in scan_paths(events) {
            info!("Scanning path: {scan_path:?}");
            let folder = self.scan_path_recursive(&scan_path);
            if folder.is_some() {
                info!("Node changed: {folder:?}");
            }
        }
        if let Some(max_event_id) = max_event_id {
            self.update_last_event_id(max_event_id);
        }
        Ok(())
    }
}

fn path_segment_matches(name: &str, segment: &OsStr, case_insensitive: bool) -> bool {
    if case_insensitive {
        segment.eq_ignore_ascii_case(name)
    } else {
        OsStr::new(name) == segment
    }
}

/// Compute the minimal set of paths that must be rescanned for a batch of FsEvents.
///
/// Goals:
/// 1. Filter out events that do not require incremental rescans (e.g. `ReScan` / `Nop` variants
///    such as RootChanged or HistoryDone). Higher-level logic either rebuilds the cache or simply
///    updates the event id for those.
/// 2. Keep only `ScanType::SingleNode` and `ScanType::Folder` paths.
/// 3. Deduplicate ancestors and descendants:
///    - Skip a path if it is already covered by an ancestor (`path.starts_with(ancestor)`).
///    - When inserting an ancestor, remove all of its descendants that were previously added.
///    - Keep only a single entry for identical paths; later duplicates are considered covered.
/// 4. Return the minimal cover—the smallest set of paths whose rescans still cover every change.
///
/// Usage:
/// - `SearchCache::handle_fs_events` iterates over the returned paths and calls
///   `scan_path_recursive` on each of them to avoid redundant rescans of descendants or duplicates.
/// - High-frequency FSEvents often bubble many changes from the same subtree; merging them here
///   significantly reduces IO and metadata fetch work downstream.
///
/// Complexity:
/// - Approximately O(n log n + m * depth): sort by depth first, then scan linearly while checking
///   ancestors.
/// - If we ever need additional speed we can explore trie/prefix-tree structures.
///
/// Corner cases:
/// - Empty input → returns an empty `Vec`.
/// - Duplicate identical paths → only one is kept (later duplicates are skipped via `starts_with`).
/// - Child path seen before its ancestor → the ancestor replaces all children, so only the ancestor remains.
/// - Ancestor seen before its child → the child is skipped.
/// - Sibling paths never interfere with each other and are all kept.
/// - Paths that merely share prefixes (e.g. `/foo/bar` vs `/foo/barista`) are both retained because
///   `Path::starts_with` compares path components.
/// - Folder and `SingleNode` events participate together; we only look at the hierarchy.
/// - `PathBuf` values are compared as-is without normalisation, so symlinks are left untouched—the
///   caller must provide consistent inputs.
///
/// Result:
/// - Local benchmarks skipped rescans for 173,034 events out of 415,449.
fn scan_paths(events: Vec<FsEvent>) -> Vec<PathBuf> {
    let mut candidates: Vec<(PathBuf, usize)> = events
        .into_iter()
        .filter(|event| {
            // Sometimes there are ridiculous events assuming dir as file, so we always scan them as folder
            matches!(
                event.flag.scan_type(),
                ScanType::SingleNode | ScanType::Folder
            )
        })
        .map(|event| {
            let path = event.path;
            let depth = path_depth(&path);
            (path, depth)
        })
        .collect();

    candidates.sort_unstable_by(|(path_a, depth_a), (path_b, depth_b)| {
        depth_a.cmp(depth_b).then_with(|| path_a.cmp(path_b))
    });
    candidates.dedup_by(|(path_a, _), (path_b, _)| path_a == path_b);

    let mut selected = Vec::with_capacity(candidates.len());
    let mut selected_set = HashSet::with_capacity(candidates.len());
    for (path, _) in candidates {
        if has_selected_ancestor(&path, &selected_set) {
            continue;
        }
        selected_set.insert(path.clone());
        selected.push(path);
    }
    selected
}

fn path_depth(path: &Path) -> usize {
    path.components().count()
}

fn has_selected_ancestor(path: &Path, selected: &HashSet<PathBuf>) -> bool {
    if selected.is_empty() {
        return false;
    }
    if selected.contains(path) {
        return true;
    }
    let mut ancestor = path.to_path_buf();
    while ancestor.pop() {
        if selected.contains(&ancestor) {
            return true;
        }
    }
    false
}

/// Error type for `SearchCache::handle_fs_event`.
#[derive(Debug)]
pub enum HandleFSEError {
    /// Full rescan is required.
    Rescan,
}

/// Note: This function is expected to be called with WalkData which metadata is not fetched.
fn construct_node_slab_name_index(
    parent: Option<SlabIndex>,
    node: &Node,
    slab: &mut ThinSlab<SlabNode>,
    name_index: &mut NameIndex,
) -> SlabIndex {
    let metadata = match node.metadata {
        Some(metadata) => SlabNodeMetadataCompact::some(metadata),
        None => SlabNodeMetadataCompact::none(),
    };
    let name = NAME_POOL.push(&node.name);
    let slab_node = SlabNode::new(parent, name, metadata);
    let index = slab.insert(slab_node);
    unsafe {
        // SAFETY: fswalk sorts each directory's children by name before we recurse,
        // so this preorder traversal visits nodes in lexicographic path order.
        name_index.add_index_ordered(name, index);
    }
    slab[index].children = node
        .children
        .iter()
        .map(|node| construct_node_slab_name_index(Some(index), node, slab, name_index))
        .collect();
    index
}

impl SearchCache {
    /// ATTENTION: This function doesn't remove existing node, you should remove it
    /// before creating the new subtree, or the old subtree nodes will be dangling.
    ///
    /// ATTENTION1: This function should only called with Node fetched with metadata.
    fn create_node_slab_update_name_index_and_name_pool(
        &mut self,
        parent: Option<SlabIndex>,
        node: &Node,
    ) -> SlabIndex {
        let metadata = match node.metadata {
            Some(metadata) => SlabNodeMetadataCompact::some(metadata),
            // This function should only be called with Node fetched with metadata
            None => SlabNodeMetadataCompact::unaccessible(),
        };
        let name = NAME_POOL.push(&node.name);
        let slab_node = SlabNode::new(parent, name, metadata);
        let index = self.push_node(slab_node);
        self.file_nodes[index].children = node
            .children
            .iter()
            .map(|node| self.create_node_slab_update_name_index_and_name_pool(Some(index), node))
            .collect::<ThinVec<_>>();
        index
    }
}

pub static NAME_POOL: LazyLock<NamePool> = LazyLock::new(NamePool::new);

fn require_folder_expr(expr: Expr) -> Expr {
    let folder_filter = Expr::Term(Term::Filter(Filter {
        kind: FilterKind::Folder,
        argument: None,
    }));

    match expr {
        Expr::Empty => folder_filter,
        expr => Expr::And(vec![expr, folder_filter]),
    }
}

#[cfg(test)]
#[path = "cache/tests/mod.rs"]
mod test_helpers;
