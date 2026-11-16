use super::*;
use crate::query::CONTENT_BUFFER_BYTES;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempdir::TempDir;

#[test]
fn test_wildcard_search_case_sensitivity() {
    let temp_dir = TempDir::new("test_wildcard_search_case_sensitivity").unwrap();
    let dir = temp_dir.path();

    fs::File::create(dir.join("AlphaOne.md")).unwrap();
    fs::File::create(dir.join("alphaTwo.md")).unwrap();
    fs::File::create(dir.join("beta.txt")).unwrap();

    let mut cache = SearchCache::walk_fs(dir);

    let opts = SearchOptions {
        case_insensitive: false,
    };
    let indices =
        guard_indices(cache.search_with_options("alpha*.md", opts, CancellationToken::noop()));
    let nodes = cache.expand_file_nodes(&indices);
    assert_eq!(nodes.len(), 1);
    assert!(nodes[0].path.ends_with("alphaTwo.md"));

    let opts = SearchOptions {
        case_insensitive: true,
    };
    let indices =
        guard_indices(cache.search_with_options("alpha*.md", opts, CancellationToken::noop()));
    let nodes = cache.expand_file_nodes(&indices);
    assert_eq!(nodes.len(), 2);
    assert!(nodes.iter().any(|node| node.path.ends_with("AlphaOne.md")));
    assert!(nodes.iter().any(|node| node.path.ends_with("alphaTwo.md")));
}

#[test]
fn content_filter_matches_file_bodies() {
    let temp_dir = TempDir::new("content_filter_matches_file_bodies").unwrap();
    let dir = temp_dir.path();

    fs::write(dir.join("notes.txt"), b"rust memchr finder\nsecond line").unwrap();
    fs::write(dir.join("other.txt"), b"nothing to see here").unwrap();

    let mut cache = SearchCache::walk_fs(dir);
    let opts = SearchOptions {
        case_insensitive: false,
    };
    let indices =
        guard_indices(cache.search_with_options("content:memchr", opts, CancellationToken::noop()));
    assert_eq!(indices.len(), 1);
    let nodes = cache.expand_file_nodes(&indices);
    assert!(nodes[0].path.ends_with("notes.txt"));

    let opts = SearchOptions {
        case_insensitive: true,
    };
    let insensitive =
        guard_indices(cache.search_with_options("content:MEMCHR", opts, CancellationToken::noop()));
    assert_eq!(insensitive.len(), 1);
    let nodes = cache.expand_file_nodes(&insensitive);
    assert!(nodes[0].path.ends_with("notes.txt"));
}

#[test]
fn content_filter_matches_across_chunks() {
    let temp_dir = TempDir::new("content_filter_matches_across_chunks").unwrap();
    let dir = temp_dir.path();

    let mut payload = vec![b'a'; CONTENT_BUFFER_BYTES.saturating_sub(1)];
    payload.extend_from_slice(b"XYZ");
    payload.extend(std::iter::repeat_n(b'a', 32));
    fs::write(dir.join("large.bin"), &payload).unwrap();

    let mut cache = SearchCache::walk_fs(dir);
    let opts = SearchOptions {
        case_insensitive: false,
    };
    let indices =
        guard_indices(cache.search_with_options("content:XYZ", opts, CancellationToken::noop()));
    assert_eq!(indices.len(), 1);
    let nodes = cache.expand_file_nodes(&indices);
    assert!(nodes.iter().any(|node| node.path.ends_with("large.bin")));
}

#[test]
fn content_filter_single_byte_respects_case_sensitivity() {
    let temp_dir = TempDir::new("content_filter_single_byte_respects_case_sensitivity").unwrap();
    let dir = temp_dir.path();

    fs::write(dir.join("letters.txt"), b"AaBb").unwrap();

    let mut cache = SearchCache::walk_fs(dir);

    let insensitive = guard_indices(cache.search_with_options(
        "content:a",
        SearchOptions {
            case_insensitive: true,
        },
        CancellationToken::noop(),
    ));
    assert_eq!(insensitive.len(), 1);

    let sensitive = guard_indices(cache.search_with_options(
        "content:a",
        SearchOptions {
            case_insensitive: false,
        },
        CancellationToken::noop(),
    ));
    assert_eq!(sensitive.len(), 1); // File contains lowercase 'a'

    // But searching for uppercase 'A' case-sensitively should also work
    let sensitive_upper = guard_indices(cache.search_with_options(
        "content:A",
        SearchOptions {
            case_insensitive: false,
        },
        CancellationToken::noop(),
    ));
    assert_eq!(sensitive_upper.len(), 1);

    // Searching for 'z' should return nothing
    let no_match = guard_indices(cache.search_with_options(
        "content:z",
        SearchOptions {
            case_insensitive: false,
        },
        CancellationToken::noop(),
    ));
    assert!(no_match.is_empty());
}

#[test]
fn content_filter_matches_at_buffer_boundary() {
    let temp_dir = TempDir::new("content_filter_matches_at_buffer_boundary").unwrap();
    let dir = temp_dir.path();

    // Place the needle so that it starts at the final byte of the first buffer
    // read and finishes in the next read.
    let mut payload = vec![b'a'; CONTENT_BUFFER_BYTES.saturating_sub(1)];
    payload.push(b'X'); // last byte of first chunk
    payload.extend_from_slice(b"YZ"); // spans into second chunk
    payload.extend(std::iter::repeat_n(b'a', 32));
    fs::write(dir.join("boundary.bin"), &payload).unwrap();

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "content:XYZ",
        SearchOptions {
            case_insensitive: false,
        },
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 1);
    let nodes = cache.expand_file_nodes(&indices);
    assert!(nodes.iter().any(|node| node.path.ends_with("boundary.bin")));
}

#[test]
fn content_filter_matches_when_needle_near_buffer_size() {
    let temp_dir = TempDir::new("content_filter_matches_when_needle_near_buffer_size").unwrap();
    let dir = temp_dir.path();

    // Build a long needle that exceeds the base buffer size, then place it once.
    let needle_len = CONTENT_BUFFER_BYTES + 8;
    let needle: String = std::iter::repeat_n('N', needle_len).collect();
    let needle_bytes = needle.as_bytes();

    let mut payload = Vec::new();
    payload.extend(std::iter::repeat_n(b'a', 16));
    payload.extend_from_slice(needle_bytes);
    payload.extend(std::iter::repeat_n(b'b', 16));
    fs::write(dir.join("long_needle.bin"), &payload).unwrap();

    let mut cache = SearchCache::walk_fs(dir);
    let query = format!("content:{needle}");
    let indices = guard_indices(cache.search_with_options(
        &query,
        SearchOptions {
            case_insensitive: false,
        },
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 1);
    let nodes = cache.expand_file_nodes(&indices);
    assert!(
        nodes
            .iter()
            .any(|node| node.path.ends_with("long_needle.bin"))
    );
}

#[test]
fn test_search_empty_cancelled_returns_none() {
    let temp_dir = TempDir::new("search_empty_cancelled").unwrap();
    fs::File::create(temp_dir.path().join("alpha.txt")).unwrap();
    let cache = SearchCache::walk_fs(temp_dir.path());

    let token = CancellationToken::new_search();
    let _ = CancellationToken::new_search();

    assert!(cache.search_empty(token).is_none());
}

#[test]
fn test_search_with_options_cancelled_returns_none() {
    let temp_dir = TempDir::new("search_with_options_cancelled").unwrap();
    fs::File::create(temp_dir.path().join("file_a.txt")).unwrap();
    let mut cache = SearchCache::walk_fs(temp_dir.path());

    let token = CancellationToken::new_search();
    let _ = CancellationToken::new_search();

    let result = cache.search_with_options(
        "file_a",
        SearchOptions {
            case_insensitive: false,
        },
        token,
    );
    assert!(matches!(result, Ok(SearchOutcome { nodes: None, .. })));
}

#[test]
fn alternate_normalization_query_ascii_returns_none() {
    let temp_dir = TempDir::new("alternate_normalization_query_ascii_returns_none").unwrap();
    let cache = SearchCache::walk_fs(temp_dir.path());

    assert_eq!(
        cache.alternate_normalization_query("office/report.txt"),
        None
    );
}

#[test]
fn alternate_normalization_query_normalization_inert_unicode_returns_none() {
    let temp_dir =
        TempDir::new("alternate_normalization_query_normalization_inert_unicode_returns_none")
            .unwrap();
    let cache = SearchCache::walk_fs(temp_dir.path());

    // CJK characters are normalization-inert in this context.
    assert_eq!(cache.alternate_normalization_query("文件/项目"), None);
}

#[test]
fn alternate_normalization_query_nfc_input_returns_nfd_variant() {
    let temp_dir =
        TempDir::new("alternate_normalization_query_nfc_input_returns_nfd_variant").unwrap();
    let cache = SearchCache::walk_fs(temp_dir.path());

    let nfc = "B\u{00FC}ro/rechnung.txt";
    let nfd = "Bu\u{0308}ro/rechnung.txt";
    assert_eq!(
        cache.alternate_normalization_query(nfc),
        Some(nfd.to_string())
    );
}

#[test]
fn alternate_normalization_query_nfd_input_returns_nfc_variant() {
    let temp_dir =
        TempDir::new("alternate_normalization_query_nfd_input_returns_nfc_variant").unwrap();
    let cache = SearchCache::walk_fs(temp_dir.path());

    let nfd = "Bu\u{0308}ro/rechnung.txt";
    let nfc = "B\u{00FC}ro/rechnung.txt";
    assert_eq!(
        cache.alternate_normalization_query(nfd),
        Some(nfc.to_string())
    );
}

#[test]
fn alternate_normalization_query_noncanonical_combining_order_is_reordered() {
    let temp_dir =
        TempDir::new("alternate_normalization_query_noncanonical_combining_order_is_reordered")
            .unwrap();
    let cache = SearchCache::walk_fs(temp_dir.path());

    let noncanonical = "a\u{0302}\u{0323}";
    let canonical_nfd = "a\u{0323}\u{0302}";
    assert_eq!(
        cache.alternate_normalization_query(noncanonical),
        Some(canonical_nfd.to_string())
    );
}

#[test]
fn search_with_options_merges_highlights_from_secondary_normalization_pass() {
    let temp_dir =
        TempDir::new("search_with_options_merges_highlights_from_secondary_normalization_pass")
            .unwrap();
    let root = temp_dir.path();
    let nfd_dir = "Bu\u{0308}ro";
    fs::create_dir_all(root.join(nfd_dir)).unwrap();
    fs::File::create(root.join(nfd_dir).join("angebot.pdf")).unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let result = cache
        .search_with_options(
            "B\u{00FC}ro",
            SearchOptions {
                case_insensitive: false,
            },
            CancellationToken::noop(),
        )
        .expect("search should succeed");

    let nodes = result
        .nodes
        .expect("noop cancellation token should not cancel");
    assert!(
        !nodes.is_empty(),
        "secondary normalization pass should match NFD path"
    );
    assert!(
        result.highlights.contains(&"büro".to_string()),
        "highlights should keep primary NFC form"
    );
    assert!(
        result.highlights.contains(&"bu\u{0308}ro".to_string()),
        "highlights should include secondary NFD form"
    );
}

#[test]
fn test_query_files_cancelled_returns_none() {
    let temp_dir = TempDir::new("query_files_cancelled").unwrap();
    fs::File::create(temp_dir.path().join("item.txt")).unwrap();
    let mut cache = SearchCache::walk_fs(temp_dir.path());

    let token = CancellationToken::new_search();
    let _ = CancellationToken::new_search();

    let result = cache.query_files("item.txt", token);
    assert!(matches!(result, Ok(None)));
}

#[test]
fn test_handle_fs_event_removal() {
    let temp_dir = TempDir::new("test_events").expect("Failed to create temp directory");
    let temp_path = temp_dir.path();
    fs::File::create(temp_path.join("new_file.txt")).expect("Failed to create file");

    let mut cache = SearchCache::walk_fs(temp_dir.path());

    assert_eq!(cache.file_nodes.len(), 2 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 2 + depth(temp_path));

    fs::remove_file(temp_path.join("new_file.txt")).expect("Failed to remove file");

    let mock_events = vec![FsEvent {
        path: temp_path.join("new_file.txt"),
        id: cache.last_event_id + 1,
        flag: EventFlag::ItemRemoved,
    }];

    cache.handle_fs_events(mock_events).unwrap();

    // Though the file in fsevents removed, we should still preserve it since it exists on disk.
    assert_eq!(cache.file_nodes.len(), 1 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 1 + depth(temp_path));
    assert_eq!(cache.search("new_file.txt").unwrap().len(), 0);
}

#[test]
#[ignore]
fn test_handle_fs_event_simulator() {
    let instant = std::time::Instant::now();
    let root = Path::new("/Library/Developer/CoreSimulator");
    let mut cache = SearchCache::walk_fs(root);
    let mut event_id = cache.last_event_id + 1;
    println!(
        "Cache size: {}, process time: {:?}",
        cache.file_nodes.len(),
        instant.elapsed()
    );
    // test speed of handling fs event
    loop {
        let instant = std::time::Instant::now();
        let mock_events = vec![FsEvent {
            path: PathBuf::from("/Library/Developer/CoreSimulator/Volumes/iOS_23A343"),
            id: event_id,
            flag: EventFlag::ItemCreated,
        }];

        cache.handle_fs_events(mock_events).unwrap();
        event_id += 1;
        println!(
            "Event id: {}, process time: {:?}",
            cache.last_event_id,
            instant.elapsed()
        );
    }
}

#[test]
fn test_handle_fs_event_removal_fake() {
    let temp_dir = TempDir::new("test_events").expect("Failed to create temp directory");
    let temp_path = temp_dir.path();
    let mut cache = SearchCache::walk_fs(temp_dir.path());

    assert_eq!(cache.file_nodes.len(), 1 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 1 + depth(temp_path));

    fs::File::create(temp_path.join("new_file.txt")).expect("Failed to create file");

    let mock_events = vec![FsEvent {
        path: temp_path.join("new_file.txt"),
        id: cache.last_event_id + 1,
        flag: EventFlag::ItemRemoved,
    }];

    cache.handle_fs_events(mock_events).unwrap();

    // Though the file in fsevents removed, we should still preserve it since it exists on disk.
    assert_eq!(cache.file_nodes.len(), 2 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 2 + depth(temp_path));
    assert_eq!(cache.search("new_file.txt").unwrap().len(), 1);
}

#[test]
fn test_handle_fs_event_add_and_removal() {
    let temp_dir = TempDir::new("test_events").expect("Failed to create temp directory");
    let temp_path = temp_dir.path();
    let mut cache = SearchCache::walk_fs(temp_dir.path());

    assert_eq!(cache.file_nodes.len(), 1 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 1 + depth(temp_path));

    fs::File::create(temp_path.join("new_file.txt")).expect("Failed to create file");

    let mock_events = vec![
        FsEvent {
            path: temp_path.join("new_file.txt"),
            id: cache.last_event_id + 1,
            flag: EventFlag::ItemCreated,
        },
        FsEvent {
            path: temp_path.join("new_file.txt"),
            id: cache.last_event_id + 1,
            flag: EventFlag::ItemRemoved,
        },
    ];

    cache.handle_fs_events(mock_events).unwrap();

    // Though the file in fsevents removed, we should still preserve it since it exists on disk.
    assert_eq!(cache.file_nodes.len(), 2 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 2 + depth(temp_path));
    assert_eq!(cache.search("new_file.txt").unwrap().len(), 1);
}

#[test]
fn test_handle_fs_event_rescan0() {
    let temp_dir = TempDir::new("test_events").expect("Failed to create temp directory");
    let temp_path = temp_dir.path();
    fs::File::create(temp_path.join("new_file.txt")).expect("Failed to create file");
    fs::File::create(temp_path.join("new_file2.txt")).expect("Failed to create file");
    fs::File::create(temp_path.join("new_file3.txt")).expect("Failed to create file");
    fs::create_dir_all(temp_path.join("src/foo")).expect("Failed to create dir");
    fs::File::create(temp_path.join("src/foo/good.rs")).expect("Failed to create file");
    let mut cache = SearchCache::walk_fs(temp_dir.path());

    assert_eq!(cache.file_nodes.len(), 7 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 7 + depth(temp_path));

    let mock_events = vec![FsEvent {
        path: temp_path.to_path_buf(),
        id: cache.last_event_id + 1,
        flag: EventFlag::RootChanged,
    }];

    cache.handle_fs_events(mock_events).unwrap_err();

    assert_eq!(cache.file_nodes.len(), 7 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 7 + depth(temp_path));
    assert_eq!(cache.search("new_file").unwrap().len(), 3);
    assert_eq!(cache.search("good.rs").unwrap().len(), 1);
    assert_eq!(cache.search("foo").unwrap().len(), 1);
}

#[test]
fn test_handle_fs_event_rescan1() {
    let temp_dir = TempDir::new("test_events").expect("Failed to create temp directory");
    let temp_path = temp_dir.path();
    let mut cache = SearchCache::walk_fs(temp_dir.path());

    assert_eq!(cache.file_nodes.len(), 1 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 1 + depth(temp_path));

    fs::File::create(temp_path.join("new_file.txt")).expect("Failed to create file");
    fs::File::create(temp_path.join("new_file2.txt")).expect("Failed to create file");
    fs::File::create(temp_path.join("new_file3.txt")).expect("Failed to create file");
    fs::create_dir_all(temp_path.join("src/foo")).expect("Failed to create dir");
    fs::File::create(temp_path.join("src/foo/good.rs")).expect("Failed to create file");

    let mock_events = vec![FsEvent {
        path: temp_path.to_path_buf(),
        id: cache.last_event_id + 1,
        flag: EventFlag::RootChanged,
    }];

    cache.handle_fs_events(mock_events).unwrap_err();

    // Rescan is required
    assert_eq!(cache.file_nodes.len(), 1 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 1 + depth(temp_path));
}

#[test]
fn test_handle_fs_event_rescan_by_modify() {
    let temp_dir = TempDir::new("test_events").expect("Failed to create temp directory");
    let temp_path = temp_dir.path();
    let mut cache = SearchCache::walk_fs(temp_dir.path());

    assert_eq!(cache.file_nodes.len(), 1 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 1 + depth(temp_path));

    fs::File::create(temp_path.join("new_file.txt")).expect("Failed to create file");
    fs::File::create(temp_path.join("new_file2.txt")).expect("Failed to create file");
    fs::File::create(temp_path.join("new_file3.txt")).expect("Failed to create file");
    fs::create_dir_all(temp_path.join("src/foo")).expect("Failed to create dir");
    fs::File::create(temp_path.join("src/foo/good.rs")).expect("Failed to create file");

    let mock_events = vec![FsEvent {
        path: temp_path.to_path_buf(),
        id: cache.last_event_id + 1,
        flag: EventFlag::ItemModified,
    }];

    cache.handle_fs_events(mock_events).unwrap_err();

    assert_eq!(cache.file_nodes.len(), 1 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 1 + depth(temp_path));
}

#[test]
fn test_handle_fs_event_dir_removal0() {
    let temp_dir = TempDir::new("test_events").expect("Failed to create temp directory");
    let temp_path = temp_dir.path();
    fs::create_dir_all(temp_path.join("Cargo.toml")).expect("Failed to create file");
    fs::File::create(temp_path.join("new_file.txt")).expect("Failed to create file");
    fs::File::create(temp_path.join("new_file2.txt")).expect("Failed to create file");
    fs::File::create(temp_path.join("new_file3.txt")).expect("Failed to create file");
    fs::create_dir_all(temp_path.join("src/foo")).expect("Failed to create dir");
    fs::File::create(temp_path.join("src/foo/good.rs")).expect("Failed to create file");
    fs::File::create(temp_path.join("src/foo.rs")).expect("Failed to create file");
    fs::File::create(temp_path.join("src/lib.rs")).expect("Failed to create file");
    fs::File::create(temp_path.join("src/boo.rs")).expect("Failed to create file");
    let mut cache = SearchCache::walk_fs(temp_dir.path());

    assert_eq!(cache.file_nodes.len(), 11 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 11 + depth(temp_path));
    assert_eq!(cache.search("src").unwrap().len(), 1);
    assert_eq!(cache.search("new_file").unwrap().len(), 3);
    assert_eq!(cache.search("good.rs").unwrap().len(), 1);
    assert_eq!(cache.search("foo").unwrap().len(), 2);
    assert_eq!(cache.search("oo.rs/").unwrap().len(), 2);
    assert_eq!(cache.search("oo").unwrap().len(), 4);

    fs::remove_dir_all(temp_path.join("src")).expect("Failed to remove dir");

    let mock_events = vec![FsEvent {
        path: temp_path.join("src"),
        id: cache.last_event_id + 1,
        flag: EventFlag::ItemRemoved | EventFlag::ItemIsDir,
    }];

    cache.handle_fs_events(mock_events).unwrap();

    assert_eq!(cache.file_nodes.len(), 5 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 5 + depth(temp_path));
    assert_eq!(cache.search("src").unwrap().len(), 0);
    assert_eq!(cache.search("new_file").unwrap().len(), 3);
    assert_eq!(cache.search("good.rs").unwrap().len(), 0);
    assert_eq!(cache.search("foo").unwrap().len(), 0);
    assert_eq!(cache.search("/foo").unwrap().len(), 0);
}

#[test]
fn test_handle_fs_event_dir_removal_triggered_by_subdir_event() {
    let temp_dir = TempDir::new("test_events").expect("Failed to create temp directory");
    let temp_path = temp_dir.path();
    fs::create_dir_all(temp_path.join("Cargo.toml")).expect("Failed to create file");
    fs::File::create(temp_path.join("new_file.txt")).expect("Failed to create file");
    fs::File::create(temp_path.join("new_file2.txt")).expect("Failed to create file");
    fs::File::create(temp_path.join("new_file3.txt")).expect("Failed to create file");
    fs::create_dir_all(temp_path.join("src/foo")).expect("Failed to create dir");
    fs::File::create(temp_path.join("src/foo/good.rs")).expect("Failed to create file");
    fs::File::create(temp_path.join("src/foo.rs")).expect("Failed to create file");
    fs::File::create(temp_path.join("src/lib.rs")).expect("Failed to create file");
    fs::File::create(temp_path.join("src/boo.rs")).expect("Failed to create file");
    let mut cache = SearchCache::walk_fs(temp_dir.path());

    assert_eq!(cache.file_nodes.len(), 11 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 11 + depth(temp_path));
    assert_eq!(cache.search("src").unwrap().len(), 1);
    assert_eq!(cache.search("new_file").unwrap().len(), 3);
    assert_eq!(cache.search("good.rs").unwrap().len(), 1);
    assert_eq!(cache.search("foo").unwrap().len(), 2);
    assert_eq!(cache.search("oo.rs/").unwrap().len(), 2);
    assert_eq!(cache.search("oo").unwrap().len(), 4);

    fs::remove_dir_all(temp_path.join("src")).expect("Failed to remove dir");

    let mock_events = vec![FsEvent {
        path: temp_path.join("src/foo"),
        id: cache.last_event_id + 1,
        flag: EventFlag::ItemRemoved | EventFlag::ItemIsDir,
    }];

    cache.handle_fs_events(mock_events).unwrap();

    assert_eq!(cache.file_nodes.len(), 9 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 9 + depth(temp_path));
    assert_eq!(cache.search("src").unwrap().len(), 1);
    assert_eq!(cache.search("new_file").unwrap().len(), 3);
    assert_eq!(cache.search("good.rs").unwrap().len(), 0);
    assert_eq!(cache.search("foo").unwrap().len(), 1);
    assert_eq!(cache.search("/foo").unwrap().len(), 1);
    assert_eq!(cache.search("oo.rs/").unwrap().len(), 2);
    assert_eq!(cache.search("oo").unwrap().len(), 2);
}

#[test]
fn test_walk_fs_new_metadata_is_always_none() {
    let temp_dir = TempDir::new("test_walk_fs_new_meta").expect("Failed to create temp directory");
    let root_path = temp_dir.path();

    fs::File::create(root_path.join("file1.txt")).expect("Failed to create file1.txt");
    fs::create_dir(root_path.join("subdir1")).expect("Failed to create subdir1");
    fs::File::create(root_path.join("subdir1/file2.txt")).expect("Failed to create file1.txt");

    let mut cache = SearchCache::walk_fs(root_path);

    // Directory nodes should always carry metadata.
    assert!(cache.file_nodes[cache.file_nodes.root()].metadata.is_some());

    // Check metadata for a file node
    let file_nodes = cache
        .search("file1.txt")
        .expect("Search for file1.txt failed");
    assert_eq!(file_nodes.len(), 1, "Expected 1 node for file1.txt");
    let file_node_idx = file_nodes.into_iter().next().unwrap();
    // File nodes should always have `metadata` set to `None`.
    assert!(
        cache.file_nodes[file_node_idx].metadata.is_none(),
        "Metadata for file node created by walk_fs_new should be None"
    );

    // Check metadata for a file node
    let file_nodes = cache
        .search("file2.txt")
        .expect("Search for file1.txt failed");
    assert_eq!(file_nodes.len(), 1);
    let file_node_idx = file_nodes.into_iter().next().unwrap();
    // File nodes should always have `metadata` set to `None`.
    assert!(
        cache.file_nodes[file_node_idx].metadata.is_none(),
        "Metadata for file node created by walk_fs_new should be None"
    );

    // Check metadata for a subdirectory node
    let dir_nodes = cache.search("subdir1").expect("Search for subdir1 failed");
    assert_eq!(dir_nodes.len(), 1, "Expected 1 node for subdir1");
    let dir_node_idx = dir_nodes.into_iter().next().unwrap();
    // Directory nodes should always carry metadata.
    assert!(
        cache.file_nodes[dir_node_idx].metadata.is_some(),
        "Metadata for directory node created by walk_fs_new should be Some"
    );
}

#[test]
fn test_handle_fs_events_metadata() {
    let temp_dir = TempDir::new("test_event_meta").expect("Failed to create temp directory");
    let root_path = temp_dir.path();

    fs::File::create(root_path.join("file1.txt")).expect("Failed to create file1.txt");
    fs::create_dir(root_path.join("subdir1")).expect("Failed to create subdir1");
    fs::File::create(root_path.join("subdir1/file2.txt")).expect("Failed to create file1.txt");

    let mut cache = SearchCache::walk_fs(root_path);
    let mut last_event_id = cache.last_event_id();

    let new_file_path = root_path.join("event_file.txt");
    fs::write(&new_file_path, b"heck").expect("Failed to create event_file.txt");

    let new_file_meta_on_disk = fs::symlink_metadata(&new_file_path).unwrap();
    last_event_id += 1;

    let file_event = FsEvent {
        path: new_file_path.clone(),
        id: last_event_id,
        flag: EventFlag::ItemCreated,
    };
    cache.handle_fs_events(vec![file_event]).unwrap();

    let file_nodes = cache
        .search("event_file.txt")
        .expect("Search for event_file.txt failed");
    assert_eq!(
        file_nodes.len(),
        1,
        "Expected 1 node for event_file.txt after event"
    );
    let file_node_idx = file_nodes.into_iter().next().unwrap();
    let file_slab_meta = cache.file_nodes[file_node_idx]
        .metadata
        .as_ref()
        .expect("Metadata for event_file.txt should be populated by event handler");
    assert_eq!(
        file_slab_meta.size(),
        new_file_meta_on_disk.len() as i64,
        "Size mismatch for event_file.txt"
    );
    assert_eq!(file_slab_meta.size(), 4, "Size mismatch for event_file.txt");
    assert!(
        file_slab_meta.mtime().is_some(),
        "mtime should be populated for event_file.txt"
    );

    // Part 2: Event for a newly created directory (should populate metadata for itself and its children)
    let new_subdir_path = root_path.join("event_subdir");
    fs::create_dir(&new_subdir_path).expect("Failed to create event_subdir");

    let file_in_subdir_path = new_subdir_path.join("file_in_event_subdir.txt");
    fs::File::create(&file_in_subdir_path).expect("Failed to create file_in_event_subdir.txt");
    let file_in_subdir_meta_on_disk = fs::symlink_metadata(&file_in_subdir_path).unwrap();
    last_event_id += 1;

    let dir_event = FsEvent {
        path: new_subdir_path.clone(), // Event is for the directory
        id: last_event_id,
        flag: EventFlag::ItemCreated | EventFlag::ItemIsDir,
    };
    cache.handle_fs_events(vec![dir_event]).unwrap();

    // Check metadata for the directory itself
    let dir_nodes = cache
        .search("/event_subdir/")
        .expect("Search for event_subdir failed");
    assert_eq!(
        dir_nodes.len(),
        1,
        "Expected 1 node for event_subdir after event"
    );
    let dir_node_idx = dir_nodes.into_iter().next().unwrap();
    let dir_slab_meta = cache.file_nodes[dir_node_idx]
        .metadata
        .as_ref()
        .expect("Metadata for event_subdir should be populated by event handler");
    assert!(
        dir_slab_meta.mtime().is_some(),
        "mtime should be populated for event_subdir"
    );

    // Check metadata for the file inside the directory
    let file_in_subdir_nodes = cache
        .search("file_in_event_subdir.txt")
        .expect("Search for file_in_event_subdir.txt failed");
    assert_eq!(
        file_in_subdir_nodes.len(),
        1,
        "Expected 1 node for file_in_event_subdir.txt after event"
    );
    let file_in_subdir_node_idx = file_in_subdir_nodes.into_iter().next().unwrap();
    let file_in_subdir_slab_meta = cache.file_nodes[file_in_subdir_node_idx]
        .metadata
        .as_ref()
        .expect("Metadata for file_in_event_subdir.txt should be populated");
    assert_eq!(
        file_in_subdir_slab_meta.size(),
        file_in_subdir_meta_on_disk.len() as i64,
        "Size mismatch for file_in_event_subdir.txt"
    );
    assert!(
        file_in_subdir_slab_meta.mtime().is_some(),
        "mtime should be populated for file_in_event_subdir.txt"
    );
}

#[test]
fn test_query_files_basic_and_no_results() {
    let temp_dir = TempDir::new("test_query_files_basic").unwrap();
    let root_path = temp_dir.path();

    fs::File::create(root_path.join("file_a.txt")).unwrap();
    fs::create_dir(root_path.join("dir_b")).unwrap();
    fs::File::create(root_path.join("dir_b/file_c.md")).unwrap();

    let mut cache = SearchCache::walk_fs(root_path);

    // 1. Query for a specific file
    let results1 = query(&mut cache, "file_a.txt");
    assert_eq!(results1.len(), 1);
    assert!(
        results1[0].path.ends_with("file_a.txt"),
        "Path was: {:?}",
        results1[0].path
    );
    assert!(
        results1[0].metadata.is_none(),
        "File metadata should be None after walk_fs_new"
    );

    // 2. Query for a file in a subdirectory
    let results2 = query(&mut cache, "file_c.md");
    assert_eq!(results2.len(), 1);
    assert!(
        results2[0].path.ends_with("dir_b/file_c.md"),
        "Path was: {:?}",
        results2[0].path
    );
    assert!(results2[0].metadata.is_none());

    // 3. Query for a directory
    let results3 = query(&mut cache, "dir_b");
    assert_eq!(results3.len(), 1);
    assert!(
        results3[0].path.ends_with("dir_b"),
        "Path was: {:?}",
        results3[0].path
    );
    assert!(
        results3[0].metadata.is_some(),
        "Directory metadata should be Some after walk_fs_new"
    );

    // 4. Query with no results
    let results4 = query(&mut cache, "non_existent.zip");
    assert_eq!(results4.len(), 0);
}

#[test]
fn test_query_files_multiple_matches_and_segments() {
    let temp_dir = TempDir::new("test_query_files_multi").unwrap();
    let root_path = temp_dir.path();

    fs::File::create(root_path.join("file_a.txt")).unwrap();
    fs::File::create(root_path.join("another_file_a.log")).unwrap();
    fs::create_dir(root_path.join("dir_b")).unwrap();
    fs::File::create(root_path.join("dir_b/file_c.md")).unwrap();

    let mut cache = SearchCache::walk_fs(root_path);

    // 5. Query matching multiple files (substring)
    let results5 = query(&mut cache, "file_a");
    assert_eq!(
        results5.len(),
        2,
        "Expected to find 'file_a.txt' and 'another_file_a.log'"
    );
    let paths5: Vec<_> = results5.iter().map(|r| r.path.clone()).collect();
    assert!(paths5.iter().any(|p| p.ends_with("file_a.txt")));
    assert!(paths5.iter().any(|p| p.ends_with("another_file_a.log")));

    // 6. Query with multiple segments (path-like search)
    // "dir_b/file_c" should find "dir_b/file_c.md"
    let results6 = query(&mut cache, "dir_b/file_c");
    assert_eq!(results6.len(), 1);
    assert!(
        results6[0].path.ends_with("dir_b/file_c.md"),
        "Path was: {:?}",
        results6[0].path
    );
}

#[test]
fn test_query_files_root_directory() {
    let temp_dir = TempDir::new("test_query_files_root").unwrap();
    let root_path = temp_dir.path();
    fs::File::create(root_path.join("some_file.txt")).unwrap(); // Add a file to make cache non-trivial

    let mut cache = SearchCache::walk_fs(root_path);
    let root_dir_name = root_path.file_name().unwrap().to_str().unwrap();

    let results = query(&mut cache, root_dir_name);
    assert_eq!(results.len(), 1, "Should find the root directory itself");
    let expected_path = root_path;
    assert_eq!(
        results[0].path, expected_path,
        "Path for root query mismatch. Expected: {:?}, Got: {:?}",
        expected_path, results[0].path
    );
    assert!(
        results[0].metadata.is_some(),
        "Root directory metadata should be Some"
    );
}

#[test]
fn test_query_files_empty_query_string() {
    let temp_dir = TempDir::new("test_query_files_empty_q").unwrap();
    let mut cache = SearchCache::walk_fs(temp_dir.path());
    // Empty queries match everything.
    let result = cache.query_files("", CancellationToken::noop());
    assert!(result.is_ok(), "empty query should succeed");
}

#[test]
fn test_query_files_deep_path_construction_and_multi_segment() {
    let temp_dir = TempDir::new("test_query_deep_path").unwrap();
    let root = temp_dir.path();
    let sub1 = root.join("alpha_dir");
    let sub2 = sub1.join("beta_subdir");
    let file_in_sub2 = sub2.join("gamma_file.txt");

    fs::create_dir_all(&sub2).unwrap();
    fs::File::create(&file_in_sub2).unwrap();

    let mut cache = SearchCache::walk_fs(root);

    // Query for the deep file directly
    let results_deep_file = query(&mut cache, "gamma_file.txt");
    assert_eq!(results_deep_file.len(), 1);
    let expected_suffix_deep = "alpha_dir/beta_subdir/gamma_file.txt".to_string();
    assert!(
        results_deep_file[0].path.ends_with(&expected_suffix_deep),
        "Path was: {:?}",
        results_deep_file[0].path
    );

    // Query for intermediate directory
    let results_sub1 = query(&mut cache, "alpha_dir");
    assert_eq!(results_sub1.len(), 1);
    assert!(
        results_sub1[0].path.ends_with("alpha_dir"),
        "Path was: {:?}",
        results_sub1[0].path
    );

    // Query for nested intermediate directory
    let results_sub2 = query(&mut cache, "beta_subdir");
    assert_eq!(results_sub2.len(), 1);
    assert!(
        results_sub2[0].path.ends_with("alpha_dir/beta_subdir"),
        "Path was: {:?}",
        results_sub2[0].path
    );

    // Test multi-segment query for the deep file
    let results_multi_segment = query(&mut cache, "alpha_dir/beta_subdir/gamma_file");
    assert_eq!(results_multi_segment.len(), 1);
    assert!(
        results_multi_segment[0]
            .path
            .ends_with(&expected_suffix_deep),
        "Path was: {:?}",
        results_multi_segment[0].path
    );

    // Test multi-segment query for an intermediate directory
    let results_multi_segment_dir = query(&mut cache, "alpha_dir/beta_subdir");
    assert_eq!(results_multi_segment_dir.len(), 1);
    assert!(
        results_multi_segment_dir[0]
            .path
            .ends_with("alpha_dir/beta_subdir"),
        "Path was: {:?}",
        results_multi_segment_dir[0].path
    );
}

#[test]
fn test_multi_segment_suffix_wildcard_matches_suffix_segments() {
    let temp_dir = TempDir::new("test_query_multi_segment_suffix_wildcard").unwrap();
    let root = temp_dir.path();
    let files = [
        "alpha_direct/mid_shared/goal.txt",
        "src_alpha_dir/mid_shared/goal.txt",
        "beta_segment/mid_shared/goal.txt",
    ];
    for relative in files {
        let full = root.join(relative);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::File::create(&full).unwrap();
    }

    let mut cache = SearchCache::walk_fs(root);
    let results = query(&mut cache, "alpha*/mid_shared/goal.txt");
    let mut matched: Vec<_> = results
        .into_iter()
        .map(|node| {
            node.path
                .strip_prefix(root)
                .expect("path inside temp dir")
                .to_path_buf()
        })
        .collect();
    matched.sort();

    assert_eq!(matched.len(), 1, "expected one suffix matches");
    let expected_alpha_direct = std::path::Path::new("alpha_direct")
        .join("mid_shared")
        .join("goal.txt");
    assert!(matched.iter().any(|path| path == &expected_alpha_direct));
}

#[test]
fn test_multi_segment_exact_wildcard_matches_middle_segment() {
    let temp_dir = TempDir::new("test_query_multi_segment_exact_wildcard").unwrap();
    let root = temp_dir.path();
    let files = [
        "work-anchor/mid_release/goal.txt",
        "work-anchor/mid_research/goal.txt",
        "work-anchor/mid_exact/goal.txt",
    ];
    for relative in files {
        let full = root.join(relative);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::File::create(&full).unwrap();
    }

    let mut cache = SearchCache::walk_fs(root);
    let results = query(&mut cache, "anchor/mid_re*/goal.txt");
    let mut matched: Vec<_> = results
        .into_iter()
        .map(|node| {
            node.path
                .strip_prefix(root)
                .expect("path inside temp dir")
                .to_path_buf()
        })
        .collect();
    matched.sort();

    assert_eq!(matched.len(), 2, "expected two middle exact matches");
    let expected_release = std::path::Path::new("work-anchor")
        .join("mid_release")
        .join("goal.txt");
    let expected_research = std::path::Path::new("work-anchor")
        .join("mid_research")
        .join("goal.txt");
    assert!(matched.iter().any(|path| path == &expected_release));
    assert!(matched.iter().any(|path| path == &expected_research));
}

#[test]
fn test_multi_segment_prefix_wildcard_matches_last_segment() {
    let temp_dir = TempDir::new("test_query_multi_segment_prefix_wildcard").unwrap();
    let root = temp_dir.path();
    let files = [
        "work-anchor/mid_release/prefix-target-alpha.txt",
        "work-anchor/mid_release/prefix-target-beta.txt",
        "work-anchor/mid_release/prefix-tool.txt",
    ];
    for relative in files {
        let full = root.join(relative);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::File::create(&full).unwrap();
    }

    let mut cache = SearchCache::walk_fs(root);
    let results = query(&mut cache, "anchor/mid_release/prefix-target*");
    let mut matched: Vec<_> = results
        .into_iter()
        .map(|node| {
            node.path
                .strip_prefix(root)
                .expect("path inside temp dir")
                .to_path_buf()
        })
        .collect();
    matched.sort();

    assert_eq!(matched.len(), 2, "expected two prefix matches");
    let expected_alpha = std::path::Path::new("work-anchor")
        .join("mid_release")
        .join("prefix-target-alpha.txt");
    let expected_beta = std::path::Path::new("work-anchor")
        .join("mid_release")
        .join("prefix-target-beta.txt");
    assert!(matched.iter().any(|path| path == &expected_alpha));
    assert!(matched.iter().any(|path| path == &expected_beta));
}

#[test]
fn test_boolean_queries() {
    let temp_dir = TempDir::new("test_boolean_queries").unwrap();
    let root = temp_dir.path();
    fs::File::create(root.join("foo.txt")).unwrap();
    fs::File::create(root.join("bar.txt")).unwrap();
    fs::File::create(root.join("foobar.txt")).unwrap();

    let mut cache = SearchCache::walk_fs(root);

    let results_and = query(&mut cache, "foo bar");
    assert_eq!(results_and.len(), 1);
    assert!(
        results_and[0].path.ends_with("foobar.txt"),
        "AND query should keep files matching both terms"
    );

    let mut names_or: Vec<_> = query(&mut cache, "foo|bar")
        .into_iter()
        .filter_map(|node| {
            node.path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .collect();
    names_or.sort();
    assert!(
        names_or.contains(&"foo.txt".to_string())
            && names_or.contains(&"bar.txt".to_string())
            && names_or.contains(&"foobar.txt".to_string()),
        "OR query should include any matching term. Found: {names_or:?}"
    );

    let excluded = query(&mut cache, "!foo");
    assert!(
        excluded.iter().all(|node| !node.path.ends_with("foo.txt")),
        "NOT query should exclude foo.txt"
    );
    assert!(
        excluded.iter().any(|node| node.path.ends_with("bar.txt")),
        "NOT query should keep unrelated files"
    );
}

#[test]
fn test_type_filters() {
    let temp_dir = TempDir::new("test_type_filters").unwrap();
    let root = temp_dir.path();
    fs::create_dir(root.join("alpha_dir")).unwrap();
    fs::File::create(root.join("alpha_dir/file_a.txt")).unwrap();
    fs::File::create(root.join("beta.txt")).unwrap();

    let mut cache = SearchCache::walk_fs(root);

    let files = query(&mut cache, "file:beta");
    assert_eq!(files.len(), 1);
    assert!(files[0].path.ends_with("beta.txt"));

    let folders = query(&mut cache, "folder:alpha");
    assert_eq!(folders.len(), 1);
    assert!(folders[0].path.ends_with("alpha_dir"));
}

#[test]
fn test_extension_and_path_filters() {
    let temp_dir = TempDir::new("test_extension_filters").unwrap();
    let root = temp_dir.path();
    let nested = root.join("nested");
    fs::create_dir(&nested).unwrap();
    fs::File::create(root.join("top.txt")).unwrap();
    fs::File::create(root.join("top.md")).unwrap();
    fs::File::create(nested.join("child.txt")).unwrap();

    let mut cache = SearchCache::walk_fs(root);

    let txt_results = query(&mut cache, "ext:txt");
    let mut txt_paths: Vec<_> = txt_results
        .into_iter()
        .filter_map(|node| {
            node.path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .collect();
    txt_paths.sort();
    assert_eq!(
        txt_paths,
        vec!["child.txt".to_string(), "top.txt".to_string()]
    );

    let parent_query = format!(r#"parent:"{}""#, root.to_string_lossy());
    let direct_children = query(&mut cache, &parent_query);
    let mut child_names: Vec<_> = direct_children
        .into_iter()
        .filter_map(|node| {
            node.path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .collect();
    child_names.sort();
    assert!(
        child_names.contains(&"top.txt".to_string()) && child_names.contains(&"top.md".to_string()),
        "parent: filter should return direct children"
    );

    let infolder_query = format!(r#"infolder:"{}""#, nested.to_string_lossy());
    let infolder_results = query(&mut cache, &infolder_query);
    assert_eq!(infolder_results.len(), 1);
    assert!(infolder_results[0].path.ends_with("nested/child.txt"));
}

#[test]
fn test_query_files_metadata_consistency_after_walk_and_event() {
    let temp_dir = TempDir::new("test_query_meta_consistency").unwrap();
    let root_path = temp_dir.path();

    let file_path_walk = root_path.join("walk_file.txt");
    let dir_path_walk = root_path.join("walk_dir");
    fs::File::create(&file_path_walk).unwrap();
    fs::create_dir(&dir_path_walk).unwrap();

    let mut cache = SearchCache::walk_fs(root_path);

    // Check metadata from initial walk_fs_new
    let results_file_walk = query(&mut cache, "walk_file.txt");
    assert_eq!(results_file_walk.len(), 1);
    assert!(
        results_file_walk[0].metadata.is_none(),
        "File metadata from walk_fs_new should be None"
    );

    let results_dir_walk = query(&mut cache, "walk_dir");
    assert_eq!(results_dir_walk.len(), 1);
    assert!(
        results_dir_walk[0].metadata.is_some(),
        "Directory metadata from walk_fs_new should be Some"
    );

    // Simulate an event for a new file
    let event_file_path = root_path.join("event_added_file.txt");
    fs::write(&event_file_path, "content123").unwrap(); // content of size 10
    let last_event_id = cache.last_event_id();
    let event = FsEvent {
        path: event_file_path.clone(),
        id: last_event_id + 1,
        flag: EventFlag::ItemCreated,
    };
    cache.handle_fs_events(vec![event]).unwrap();

    let results_event_file = query(&mut cache, "event_added_file.txt");
    assert_eq!(results_event_file.len(), 1);
    let event_file_meta = results_event_file[0]
        .metadata
        .as_ref()
        .expect("File metadata should be Some after event processing");
    assert_eq!(event_file_meta.size(), 10);

    // Simulate an event for a new directory with a file in it
    let event_dir_path = root_path.join("event_added_dir");
    fs::create_dir(&event_dir_path).unwrap();
    let file_in_event_dir_path = event_dir_path.join("inner_event.dat");
    fs::write(&file_in_event_dir_path, "data").unwrap(); // content of size 4

    let last_event_id_2 = cache.last_event_id();
    let event_dir = FsEvent {
        path: event_dir_path.clone(), // Event is for the directory
        id: last_event_id_2 + 1,
        flag: EventFlag::ItemCreated | EventFlag::ItemIsDir, // scan_path_recursive will scan children
    };
    cache.handle_fs_events(vec![event_dir]).unwrap();

    let results_event_dir = query(&mut cache, "event_added_dir");
    assert_eq!(results_event_dir.len(), 1);
    assert!(
        results_event_dir[0].metadata.is_some(),
        "Dir metadata should be Some after event processing for dir"
    );

    let results_file_in_event_dir = query(&mut cache, "inner_event.dat");
    assert_eq!(results_file_in_event_dir.len(), 1);
    let inner_file_meta = results_file_in_event_dir[0]
        .metadata
        .as_ref()
        .expect("File in event-added dir metadata should be Some");
    assert_eq!(inner_file_meta.size(), 4);
}

// --- scan_paths focused tests ---
#[test]
fn test_scan_paths_empty() {
    assert!(scan_paths(vec![]).is_empty());
}

#[test]
fn test_scan_paths_only_rescan_events_kept_when_called_directly() {
    let root = PathBuf::from("/tmp/root");
    let events = vec![FsEvent {
        path: root.clone(),
        id: 1,
        flag: EventFlag::RootChanged,
    }];
    assert!(scan_paths(events).is_empty());
}

#[test]
fn test_scan_paths_history_done_filtered() {
    let p = PathBuf::from("/tmp/a");
    let events = vec![FsEvent {
        path: p,
        id: 1,
        flag: EventFlag::HistoryDone,
    }];
    // HistoryDone => ScanType::Nop
    assert!(scan_paths(events).is_empty());
}

#[test]
fn test_scan_paths_dedup_same_path() {
    let p = PathBuf::from("/tmp/a/b");
    let events = vec![
        FsEvent {
            path: p.clone(),
            id: 1,
            flag: EventFlag::ItemCreated | EventFlag::ItemIsDir,
        },
        FsEvent {
            path: p.clone(),
            id: 2,
            flag: EventFlag::ItemModified | EventFlag::ItemIsFile,
        }, // Assume the flag is incorrect; treat it as SingleNode anyway.
        FsEvent {
            path: p,
            id: 3,
            flag: EventFlag::ItemRemoved | EventFlag::ItemIsFile,
        },
    ];
    let out = scan_paths(events);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0], PathBuf::from("/tmp/a/b"));
}

#[test]
fn test_scan_paths_child_then_parent_collapses() {
    let events = vec![
        FsEvent {
            path: PathBuf::from("/t/a/b/c"),
            id: 1,
            flag: EventFlag::ItemCreated | EventFlag::ItemIsFile,
        },
        FsEvent {
            path: PathBuf::from("/t/a/b"),
            id: 2,
            flag: EventFlag::ItemModified | EventFlag::ItemIsDir,
        },
        FsEvent {
            path: PathBuf::from("/t/a"),
            id: 3,
            flag: EventFlag::ItemModified | EventFlag::ItemIsDir,
        },
    ];
    let out = scan_paths(events);
    // Expect the ancestor /t/a to absorb the whole subtree.
    assert_eq!(out, vec![PathBuf::from("/t/a")]);
}

#[test]
fn test_scan_paths_parent_then_child_skip_child() {
    let events = vec![
        FsEvent {
            path: PathBuf::from("/t/a"),
            id: 1,
            flag: EventFlag::ItemModified | EventFlag::ItemIsDir,
        },
        FsEvent {
            path: PathBuf::from("/t/a/b"),
            id: 2,
            flag: EventFlag::ItemCreated | EventFlag::ItemIsFile,
        },
        FsEvent {
            path: PathBuf::from("/t/a/b/c"),
            id: 3,
            flag: EventFlag::ItemCreated | EventFlag::ItemIsFile,
        },
    ];
    let out = scan_paths(events);
    assert_eq!(out, vec![PathBuf::from("/t/a")]);
}

#[test]
fn test_scan_paths_siblings_all_retained() {
    let events = vec![
        FsEvent {
            path: PathBuf::from("/t/a/x"),
            id: 1,
            flag: EventFlag::ItemCreated | EventFlag::ItemIsFile,
        },
        FsEvent {
            path: PathBuf::from("/t/a/y"),
            id: 2,
            flag: EventFlag::ItemCreated | EventFlag::ItemIsFile,
        },
        FsEvent {
            path: PathBuf::from("/t/a/z"),
            id: 3,
            flag: EventFlag::ItemCreated | EventFlag::ItemIsFile,
        },
    ];
    let mut out = scan_paths(events);
    out.sort();
    assert_eq!(
        out,
        vec!["/t/a/x", "/t/a/y", "/t/a/z"]
            .into_iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_scan_paths_prefix_but_not_parent() {
    // /foo/bar and /foo/barista share a prefix but are not parent/child; both should stay.
    let events = vec![
        FsEvent {
            path: PathBuf::from("/foo/bar"),
            id: 1,
            flag: EventFlag::ItemCreated | EventFlag::ItemIsDir,
        },
        FsEvent {
            path: PathBuf::from("/foo/barista"),
            id: 2,
            flag: EventFlag::ItemCreated | EventFlag::ItemIsDir,
        },
    ];
    let mut out = scan_paths(events);
    out.sort();
    assert_eq!(
        out,
        vec![PathBuf::from("/foo/bar"), PathBuf::from("/foo/barista")]
    );
}

#[test]
fn test_scan_paths_mix_folder_and_single_node() {
    // Directory creation plus file modification: the directory should absorb its child.
    let events = vec![
        FsEvent {
            path: PathBuf::from("/mix/dir/sub/file.txt"),
            id: 1,
            flag: EventFlag::ItemModified | EventFlag::ItemIsFile,
        },
        FsEvent {
            path: PathBuf::from("/mix/dir/sub"),
            id: 2,
            flag: EventFlag::ItemCreated | EventFlag::ItemIsDir,
        },
    ];
    let out = scan_paths(events);
    assert_eq!(out, vec![PathBuf::from("/mix/dir/sub")]);
}

#[test]
fn test_scan_paths_depth_then_lexicographic_ordering() {
    let events = vec![
        FsEvent {
            path: PathBuf::from("/z/child"),
            id: 1,
            flag: EventFlag::ItemCreated | EventFlag::ItemIsFile,
        },
        FsEvent {
            path: PathBuf::from("/a"),
            id: 2,
            flag: EventFlag::ItemCreated | EventFlag::ItemIsDir,
        },
        FsEvent {
            path: PathBuf::from("/m"),
            id: 3,
            flag: EventFlag::ItemCreated | EventFlag::ItemIsDir,
        },
        FsEvent {
            path: PathBuf::from("/a/child"),
            id: 4,
            flag: EventFlag::ItemCreated | EventFlag::ItemIsFile,
        },
    ];
    let out = scan_paths(events);
    assert_eq!(
        out,
        vec![
            PathBuf::from("/a"),
            PathBuf::from("/m"),
            PathBuf::from("/z/child")
        ]
    );
}

#[test]
fn test_scan_paths_handles_root_ancestor() {
    let events = vec![
        FsEvent {
            path: PathBuf::from("/foo/bar"),
            id: 1,
            flag: EventFlag::ItemCreated | EventFlag::ItemIsFile,
        },
        FsEvent {
            path: PathBuf::from("/"),
            id: 2,
            flag: EventFlag::ItemCreated | EventFlag::ItemIsDir,
        },
    ];
    let out = scan_paths(events);
    assert_eq!(out, vec![PathBuf::from("/")]);
}

#[test]
fn test_scan_paths_large_chain_collapse() {
    // Build a long chain where the ancestor arrives at the end.
    let mut events = Vec::new();
    let depth = ["a", "b", "c", "d", "e", "f"];
    for i in 0..depth.len() {
        let path = format!("/long/{}", depth[..=i].join("/"));
        events.push(FsEvent {
            path: PathBuf::from(path),
            id: i as u64,
            flag: EventFlag::ItemCreated | EventFlag::ItemIsDir,
        });
    }
    // Add the real ancestor /long.
    events.push(FsEvent {
        path: PathBuf::from("/long"),
        id: 99,
        flag: EventFlag::ItemModified | EventFlag::ItemIsDir,
    });
    let out = scan_paths(events);
    assert_eq!(out, vec![PathBuf::from("/long")]);
}
