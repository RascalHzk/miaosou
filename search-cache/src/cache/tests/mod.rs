use super::*;
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
use tempdir::TempDir;

fn depth(path: &Path) -> usize {
    path.components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count()
}

fn guard_indices(result: Result<SearchOutcome>) -> Vec<SlabIndex> {
    result
        .expect("search should succeed")
        .nodes
        .expect("noop cancellation token should not cancel")
}

fn guard_nodes(result: Result<Option<Vec<SearchResultNode>>>) -> Vec<SearchResultNode> {
    result
        .expect("query should succeed")
        .expect("noop cancellation token should not cancel")
}

fn query(cache: &mut SearchCache, query: &str) -> Vec<SearchResultNode> {
    guard_nodes(cache.query_files(query, CancellationToken::noop()))
}

fn scoped_search(
    cache: &mut SearchCache,
    directory_query: Option<&str>,
    query: Option<&str>,
) -> Vec<SearchResultNode> {
    scoped_search_with_options(
        cache,
        directory_query,
        query,
        SearchOptions::default(),
        CancellationToken::noop(),
    )
}

fn scoped_search_with_options(
    cache: &mut SearchCache,
    directory_query: Option<&str>,
    query: Option<&str>,
    options: SearchOptions,
    token: CancellationToken,
) -> Vec<SearchResultNode> {
    let indices = guard_indices(cache.search_query_with_options(
        SearchQuery {
            directory_query: directory_query.map(ToString::to_string),
            query: query.map(ToString::to_string),
        },
        options,
        token,
    ));
    cache.expand_file_nodes(&indices)
}

fn scoped_search_outcome(
    cache: &mut SearchCache,
    directory_query: Option<&str>,
    query: Option<&str>,
    token: CancellationToken,
) -> SearchOutcome {
    cache
        .search_query_with_options(
            SearchQuery {
                directory_query: directory_query.map(ToString::to_string),
                query: query.map(ToString::to_string),
            },
            SearchOptions::default(),
            token,
        )
        .expect("search should succeed")
}

fn paths_from_nodes(nodes: &[SearchResultNode]) -> Vec<String> {
    nodes
        .iter()
        .map(|node| node.path.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
}

fn assert_path_suffixes(paths: &[String], expected: &[&str]) {
    assert_eq!(
        paths.len(),
        expected.len(),
        "expected suffixes {expected:?}, got paths {paths:?}"
    );
    for suffix in expected {
        assert!(
            paths.iter().any(|path| path.ends_with(suffix)),
            "missing suffix {suffix:?} in {paths:?}"
        );
    }
}

fn assert_node_suffixes(nodes: &[SearchResultNode], expected: &[&str]) {
    let paths = paths_from_nodes(nodes);
    assert_path_suffixes(&paths, expected);
}

fn bytes_with_prefix(prefix: &str, len: usize) -> Vec<u8> {
    let mut bytes = prefix.as_bytes().to_vec();
    bytes.resize(len, b'x');
    bytes
}

fn write_sized_file(path: &Path, prefix: &str, len: usize) {
    fs::write(path, bytes_with_prefix(prefix, len)).unwrap();
}

fn build_base_filter_fixture(name: &str) -> (TempDir, PathBuf, SearchCache) {
    let temp_dir = TempDir::new(name).unwrap();
    let root = temp_dir.path().to_path_buf();
    for dir in [
        "Work/Docs/Nested",
        "Work/Docs/FolderMatch",
        "Work/Images",
        "Personal/Docs/Nested",
        "Personal/Docs/FolderMatch",
        "Personal/Images",
    ] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }

    write_sized_file(
        &root.join("Work/Docs/report.md"),
        "needle report document",
        4096,
    );
    write_sized_file(
        &root.join("Personal/Docs/report.md"),
        "needle report document",
        4096,
    );
    write_sized_file(&root.join("Work/Docs/notes.txt"), "needle notes", 2048);
    write_sized_file(&root.join("Personal/Docs/notes.txt"), "needle notes", 2048);
    write_sized_file(&root.join("Work/Docs/archive.md"), "needle archive", 4096);
    write_sized_file(
        &root.join("Personal/Docs/archive.md"),
        "needle archive",
        4096,
    );
    write_sized_file(&root.join("Work/Docs/draft.pdf"), "needle draft", 512);
    write_sized_file(&root.join("Personal/Docs/draft.pdf"), "needle draft", 512);
    write_sized_file(
        &root.join("Work/Docs/Nested/nested.md"),
        "needle nested document",
        2048,
    );
    write_sized_file(
        &root.join("Personal/Docs/Nested/nested.md"),
        "needle nested document",
        2048,
    );
    write_sized_file(
        &root.join("Work/Images/report.md"),
        "needle image scope",
        4096,
    );
    write_sized_file(
        &root.join("Personal/Images/report.md"),
        "needle image scope",
        4096,
    );
    write_sized_file(&root.join("Work/Docs/song.mp3"), "needle song", 1024);
    write_sized_file(&root.join("Personal/Docs/song.mp3"), "needle song", 1024);
    write_sized_file(&root.join("Work/Docs/clip.mp4"), "needle clip", 1024);
    write_sized_file(&root.join("Personal/Docs/clip.mp4"), "needle clip", 1024);
    write_sized_file(&root.join("Work/Docs/tool.exe"), "needle tool", 1024);
    write_sized_file(&root.join("Personal/Docs/tool.exe"), "needle tool", 1024);

    let cache = SearchCache::walk_fs(&root);
    (temp_dir, root, cache)
}

fn make_node(name: &str, children: Vec<Node>) -> Node {
    Node {
        children,
        name: name.into(),
        metadata: None,
    }
}

fn make_leaf(name: &str) -> Node {
    make_node(name, vec![])
}

fn push_child(slab: &mut ThinSlab<SlabNode>, parent: SlabIndex, name: &str) -> SlabIndex {
    let idx = slab.insert(SlabNode::new(
        Some(parent),
        NAME_POOL.push(name),
        SlabNodeMetadataCompact::none(),
    ));
    slab[parent].children.push(idx);
    idx
}

fn find_node_index(cache: &SearchCache, path: &Path) -> SlabIndex {
    let path = path.strip_prefix("/").expect("absolute path");
    let mut current = cache.file_nodes.root();
    for name in path {
        let name = name.to_string_lossy();
        current = *cache.file_nodes[current]
            .children
            .iter()
            .find(|&&idx| cache.file_nodes[idx].name() == name.as_ref())
            .expect("node should exist");
    }
    current
}

fn manual_target_tree_file_nodes() -> (FileNodes, [SlabIndex; 3]) {
    let mut slab = ThinSlab::new();
    let root_idx = slab.insert(SlabNode::new(
        None,
        NAME_POOL.push("root"),
        SlabNodeMetadataCompact::none(),
    ));
    let alpha = push_child(&mut slab, root_idx, "alpha");
    let beta = push_child(&mut slab, root_idx, "beta");
    let root_target = push_child(&mut slab, root_idx, "target.txt");
    let alpha_target = push_child(&mut slab, alpha, "target.txt");
    let beta_target = push_child(&mut slab, beta, "target.txt");
    let file_nodes = FileNodes::new(
        PathBuf::from("/virtual/root"),
        Vec::new(),
        Vec::new(),
        slab,
        root_idx,
    );
    (file_nodes, [root_target, alpha_target, beta_target])
}

mod part_a;
mod part_b;
