use super::*;
use fswalk::NodeFileType;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempdir::TempDir;

#[test]
fn directory_query_only_returns_matching_directories() {
    let temp_dir = TempDir::new("directory_query_only").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Projects/Alpha/src")).unwrap();
    fs::create_dir_all(root.join("Archive/Alpha")).unwrap();
    fs::write(root.join("Projects/Alpha/src/notes.md"), b"notes").unwrap();
    fs::write(root.join("Archive/Alpha/old.md"), b"old").unwrap();
    fs::write(root.join("Alpha.txt"), b"not a directory").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let nodes = scoped_search(&mut cache, Some("Projects/Alpha"), None);
    let paths = paths_from_nodes(&nodes);

    assert_eq!(paths.len(), 1);
    assert!(paths[0].ends_with("/Projects/Alpha"));
    assert!(
        !paths
            .iter()
            .any(|path| path.ends_with("/Projects/Alpha/src"))
    );
    assert!(
        !paths
            .iter()
            .any(|path| path.ends_with("/Archive/Alpha/old.md"))
    );
    assert!(!paths.iter().any(|path| path.ends_with("/Alpha.txt")));
}

#[test]
fn directory_query_only_rejects_file_type_filters() {
    let temp_dir = TempDir::new("directory_query_only_file_filter").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Folder")).unwrap();
    fs::write(root.join("OnlyFile"), b"not a directory").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let nodes = scoped_search(&mut cache, Some("file:OnlyFile"), None);

    assert!(nodes.is_empty());
}

#[test]
fn empty_search_query_returns_all_nodes() {
    let temp_dir = TempDir::new("directory_query_none").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Projects/Alpha")).unwrap();
    fs::write(root.join("Projects/Alpha/notes.md"), b"notes").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let nodes = scoped_search(&mut cache, None, None);
    let paths = paths_from_nodes(&nodes);

    assert!(paths.iter().any(|path| path.ends_with("/Projects")));
    assert!(paths.iter().any(|path| path.ends_with("/Projects/Alpha")));
    assert!(
        paths
            .iter()
            .any(|path| path.ends_with("/Projects/Alpha/notes.md"))
    );
}

#[test]
fn empty_directory_query_is_explicit_folder_only_search() {
    let temp_dir = TempDir::new("directory_query_empty_string").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Projects/Alpha")).unwrap();
    fs::write(root.join("Projects/Alpha/notes.md"), b"notes").unwrap();
    fs::write(root.join("root.txt"), b"root").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let nodes = scoped_search(&mut cache, Some(""), None);
    let paths = paths_from_nodes(&nodes);

    assert!(paths.iter().any(|path| path.ends_with("/Projects")));
    assert!(paths.iter().any(|path| path.ends_with("/Projects/Alpha")));
    assert!(
        !paths
            .iter()
            .any(|path| path.ends_with("/Projects/Alpha/notes.md"))
    );
    assert!(!paths.iter().any(|path| path.ends_with("/root.txt")));
}

#[test]
fn whitespace_directory_query_is_not_treated_as_absent() {
    let temp_dir = TempDir::new("directory_query_whitespace_string").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Projects/Alpha")).unwrap();
    fs::write(root.join("Projects/Alpha/notes.md"), b"notes").unwrap();
    fs::write(root.join("root.txt"), b"root").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let nodes = scoped_search(&mut cache, Some("   "), None);
    let paths = paths_from_nodes(&nodes);

    assert!(paths.iter().any(|path| path.ends_with("/Projects")));
    assert!(paths.iter().any(|path| path.ends_with("/Projects/Alpha")));
    assert!(
        !paths
            .iter()
            .any(|path| path.ends_with("/Projects/Alpha/notes.md"))
    );
    assert!(!paths.iter().any(|path| path.ends_with("/root.txt")));
}

#[test]
fn query_only_runs_normal_single_pass_search() {
    let temp_dir = TempDir::new("query_only").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Work/Docs")).unwrap();
    fs::create_dir_all(root.join("Personal/Docs")).unwrap();
    fs::write(root.join("Work/Docs/report.md"), b"report").unwrap();
    fs::write(root.join("Personal/Docs/report.md"), b"report").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let nodes = scoped_search(&mut cache, None, Some("report*.md"));
    let paths = paths_from_nodes(&nodes);

    assert_eq!(paths.len(), 2);
    assert!(
        paths
            .iter()
            .any(|path| path.ends_with("/Work/Docs/report.md"))
    );
    assert!(
        paths
            .iter()
            .any(|path| path.ends_with("/Personal/Docs/report.md"))
    );
}

#[test]
fn directory_query_intersects_with_main_query() {
    let temp_dir = TempDir::new("directory_query_main_query").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Work/Docs")).unwrap();
    fs::create_dir_all(root.join("Personal/Docs")).unwrap();
    fs::write(root.join("Work/Docs/report.md"), b"report").unwrap();
    fs::write(root.join("Work/Docs/report.txt"), b"report").unwrap();
    fs::write(root.join("Personal/Docs/report.md"), b"report").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let nodes = scoped_search(&mut cache, Some("Work/Docs"), Some("report*.md"));
    let paths = paths_from_nodes(&nodes);

    assert_eq!(paths.len(), 1);
    assert!(paths[0].ends_with("/Work/Docs/report.md"));
}

#[test]
fn directory_query_scope_includes_deep_descendants_for_main_query() {
    let temp_dir = TempDir::new("directory_query_deep_descendants").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Scope/Target/one/two")).unwrap();
    fs::create_dir_all(root.join("Other/Target/one/two")).unwrap();
    fs::write(root.join("Scope/Target/one/two/deep.md"), b"deep").unwrap();
    fs::write(root.join("Other/Target/one/two/deep.md"), b"deep").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let nodes = scoped_search(&mut cache, Some("Scope/Target"), Some("deep.md"));
    let paths = paths_from_nodes(&nodes);

    assert_eq!(paths.len(), 1);
    assert!(paths[0].ends_with("/Scope/Target/one/two/deep.md"));
}

#[test]
fn search_with_options_base_limits_main_query_to_scope_nodes() {
    let temp_dir = TempDir::new("search_with_options_base_scope").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Work/Docs")).unwrap();
    fs::create_dir_all(root.join("Personal/Docs")).unwrap();
    fs::write(root.join("Work/Docs/report.md"), b"work").unwrap();
    fs::write(root.join("Personal/Docs/report.md"), b"personal").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let scope = guard_indices(cache.search_directory_scope(
        "Work/Docs",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    let indices = guard_indices(cache.search_with_options_base(
        "report*.md",
        Some(&scope),
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    let paths = paths_from_nodes(&cache.expand_file_nodes(&indices));

    assert_eq!(paths.len(), 1);
    assert!(paths[0].ends_with("/Work/Docs/report.md"));
}

#[test]
fn directory_query_with_empty_main_query_returns_scope_descendants() {
    let temp_dir = TempDir::new("directory_query_empty_main_scope").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Work/Docs/Nested")).unwrap();
    fs::create_dir_all(root.join("Personal/Docs")).unwrap();
    fs::write(root.join("Work/Docs/report.md"), b"work").unwrap();
    fs::write(root.join("Work/Docs/Nested/notes.md"), b"notes").unwrap();
    fs::write(root.join("Personal/Docs/report.md"), b"personal").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let nodes = scoped_search(&mut cache, Some("Work/Docs"), Some(""));
    let paths = paths_from_nodes(&nodes);

    assert_eq!(paths.len(), 3);
    assert!(paths.iter().any(|path| path.ends_with("/Work/Docs/Nested")));
    assert!(
        paths
            .iter()
            .any(|path| path.ends_with("/Work/Docs/report.md"))
    );
    assert!(
        paths
            .iter()
            .any(|path| path.ends_with("/Work/Docs/Nested/notes.md"))
    );
    assert!(
        !paths
            .iter()
            .any(|path| path.ends_with("/Work/Docs") || path.ends_with("/Personal/Docs"))
    );
    assert!(
        !paths
            .iter()
            .any(|path| path.ends_with("/Personal/Docs/report.md"))
    );
}

#[test]
fn directory_query_with_or_main_query_respects_scope_base() {
    let temp_dir = TempDir::new("directory_query_or_scope").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Work/Docs")).unwrap();
    fs::create_dir_all(root.join("Personal/Docs")).unwrap();
    fs::write(root.join("Work/Docs/report.md"), b"work report").unwrap();
    fs::write(root.join("Work/Docs/notes.md"), b"work notes").unwrap();
    fs::write(root.join("Personal/Docs/report.md"), b"personal report").unwrap();
    fs::write(root.join("Personal/Docs/notes.md"), b"personal notes").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let nodes = scoped_search(&mut cache, Some("Work/Docs"), Some("report.md | notes.md"));
    let paths = paths_from_nodes(&nodes);

    assert_eq!(paths.len(), 2);
    assert!(
        paths
            .iter()
            .any(|path| path.ends_with("/Work/Docs/report.md"))
    );
    assert!(
        paths
            .iter()
            .any(|path| path.ends_with("/Work/Docs/notes.md"))
    );
    assert!(!paths.iter().any(|path| path.contains("/Personal/Docs/")));
}

#[test]
fn directory_query_with_not_main_query_uses_scope_as_universe() {
    let temp_dir = TempDir::new("directory_query_not_scope").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Work/Docs")).unwrap();
    fs::create_dir_all(root.join("Personal/Docs")).unwrap();
    fs::write(root.join("Work/Docs/report.md"), b"report").unwrap();
    fs::write(root.join("Work/Docs/draft.md"), b"draft").unwrap();
    fs::write(root.join("Personal/Docs/report.md"), b"report").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let nodes = scoped_search(&mut cache, Some("Work/Docs"), Some("!draft.md"));
    let paths = paths_from_nodes(&nodes);

    assert_eq!(paths.len(), 1);
    assert!(paths[0].ends_with("/Work/Docs/report.md"));
}

#[test]
fn directory_query_with_regex_main_query_respects_scope_base() {
    let temp_dir = TempDir::new("directory_query_regex_scope").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Work/Docs")).unwrap();
    fs::create_dir_all(root.join("Personal/Docs")).unwrap();
    fs::write(root.join("Work/Docs/report-2026.md"), b"work").unwrap();
    fs::write(root.join("Work/Docs/notes.md"), b"notes").unwrap();
    fs::write(root.join("Personal/Docs/report-2026.md"), b"personal").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let nodes = scoped_search(
        &mut cache,
        Some("Work/Docs"),
        Some(r"regex:^report-\d+\.md$"),
    );
    let paths = paths_from_nodes(&nodes);

    assert_eq!(paths.len(), 1);
    assert!(paths[0].ends_with("/Work/Docs/report-2026.md"));
}

#[test]
fn directory_query_with_filter_only_main_query_uses_scope_base() {
    let temp_dir = TempDir::new("directory_query_filter_scope").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Work/Docs")).unwrap();
    fs::create_dir_all(root.join("Personal/Docs")).unwrap();
    fs::write(root.join("Work/Docs/report.md"), b"work").unwrap();
    fs::write(root.join("Work/Docs/report.txt"), b"work txt").unwrap();
    fs::write(root.join("Personal/Docs/report.md"), b"personal").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let nodes = scoped_search(&mut cache, Some("Work/Docs"), Some("ext:md"));
    let paths = paths_from_nodes(&nodes);

    assert_eq!(paths.len(), 1);
    assert!(paths[0].ends_with("/Work/Docs/report.md"));
}

#[test]
fn directory_query_with_and_main_query_keeps_filter_order_base_aware() {
    let temp_dir = TempDir::new("directory_query_and_order_scope").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Work/Docs")).unwrap();
    fs::create_dir_all(root.join("Personal/Docs")).unwrap();
    fs::write(root.join("Work/Docs/report.md"), b"work").unwrap();
    fs::write(root.join("Work/Docs/report.txt"), b"work txt").unwrap();
    fs::write(root.join("Personal/Docs/report.md"), b"personal").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    for query in ["report ext:md", "ext:md report"] {
        let nodes = scoped_search(&mut cache, Some("Work/Docs"), Some(query));
        let paths = paths_from_nodes(&nodes);

        assert_eq!(paths.len(), 1, "query {query:?}");
        assert!(
            paths[0].ends_with("/Work/Docs/report.md"),
            "query {query:?} returned {paths:?}"
        );
    }
}

#[test]
fn directory_query_with_content_filter_main_query_uses_scope_base() {
    let temp_dir = TempDir::new("directory_query_content_scope").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Work/Docs")).unwrap();
    fs::create_dir_all(root.join("Personal/Docs")).unwrap();
    fs::write(root.join("Work/Docs/report.md"), b"shared needle").unwrap();
    fs::write(root.join("Personal/Docs/report.md"), b"shared needle").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let nodes = scoped_search(&mut cache, Some("Work/Docs"), Some("content:needle"));
    let paths = paths_from_nodes(&nodes);

    assert_eq!(paths.len(), 1);
    assert!(paths[0].ends_with("/Work/Docs/report.md"));
}

#[test]
fn directory_query_with_empty_scope_base_returns_no_main_results() {
    let temp_dir = TempDir::new("directory_query_empty_scope_base").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Empty/Scope")).unwrap();
    fs::write(root.join("outside.md"), b"outside").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let nodes = scoped_search(&mut cache, Some("Empty/Scope"), Some("*"));

    assert!(nodes.is_empty());
}

#[test]
fn directory_query_with_parent_filter_main_query_respects_scope_base() {
    let temp_dir = TempDir::new("directory_query_parent_scope").unwrap();
    let root = temp_dir.path();
    let work_docs = root.join("Work/Docs");
    let personal_docs = root.join("Personal/Docs");
    fs::create_dir_all(&work_docs).unwrap();
    fs::create_dir_all(&personal_docs).unwrap();
    fs::write(work_docs.join("report.md"), b"work").unwrap();
    fs::write(personal_docs.join("report.md"), b"personal").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let personal_parent_query = format!(r#"parent:"{}""#, personal_docs.display());
    let work_parent_query = format!(r#"parent:"{}""#, work_docs.display());

    let outside_nodes = scoped_search(&mut cache, Some("Work/Docs"), Some(&personal_parent_query));
    assert!(outside_nodes.is_empty());

    let inside_nodes = scoped_search(&mut cache, Some("Work/Docs"), Some(&work_parent_query));
    let paths = paths_from_nodes(&inside_nodes);
    assert_eq!(paths.len(), 1);
    assert!(paths[0].ends_with("/Work/Docs/report.md"));
}

#[test]
fn directory_query_with_infolder_filter_main_query_respects_scope_base() {
    let temp_dir = TempDir::new("directory_query_infolder_scope").unwrap();
    let root = temp_dir.path();
    let work_docs = root.join("Work/Docs");
    let personal_docs = root.join("Personal/Docs");
    fs::create_dir_all(work_docs.join("Nested")).unwrap();
    fs::create_dir_all(personal_docs.join("Nested")).unwrap();
    fs::write(work_docs.join("Nested/report.md"), b"work").unwrap();
    fs::write(personal_docs.join("Nested/report.md"), b"personal").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let personal_query = format!(r#"infolder:"{}""#, personal_docs.display());
    let work_query = format!(r#"infolder:"{}""#, work_docs.display());

    let outside_nodes = scoped_search(&mut cache, Some("Work/Docs"), Some(&personal_query));
    assert!(outside_nodes.is_empty());

    let inside_nodes = scoped_search(&mut cache, Some("Work/Docs"), Some(&work_query));
    let paths = paths_from_nodes(&inside_nodes);
    assert_eq!(paths.len(), 2);
    assert!(paths.iter().any(|path| path.ends_with("/Work/Docs/Nested")));
    assert!(
        paths
            .iter()
            .any(|path| path.ends_with("/Work/Docs/Nested/report.md"))
    );
}

#[test]
fn directory_query_with_nosubfolders_filter_main_query_respects_scope_base() {
    let temp_dir = TempDir::new("directory_query_nosubfolders_scope").unwrap();
    let root = temp_dir.path();
    let work_docs = root.join("Work/Docs");
    let personal_docs = root.join("Personal/Docs");
    fs::create_dir_all(work_docs.join("Nested")).unwrap();
    fs::create_dir_all(&personal_docs).unwrap();
    fs::write(work_docs.join("report.md"), b"work").unwrap();
    fs::write(work_docs.join("Nested/deep.md"), b"deep").unwrap();
    fs::write(personal_docs.join("report.md"), b"personal").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let personal_query = format!(r#"nosubfolders:"{}""#, personal_docs.display());
    let work_query = format!(r#"nosubfolders:"{}""#, work_docs.display());

    let outside_nodes = scoped_search(&mut cache, Some("Work/Docs"), Some(&personal_query));
    assert!(outside_nodes.is_empty());

    let inside_nodes = scoped_search(&mut cache, Some("Work/Docs"), Some(&work_query));
    let paths = paths_from_nodes(&inside_nodes);
    assert_eq!(paths.len(), 1);
    assert!(paths[0].ends_with("/Work/Docs/report.md"));
}

#[test]
fn search_with_options_base_propagates_primary_cancellation() {
    let temp_dir = TempDir::new("search_with_options_base_cancel").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Work/Docs")).unwrap();
    fs::write(root.join("Work/Docs/report.md"), b"work").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let scope = guard_indices(cache.search_directory_scope(
        "Work/Docs",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    let token = CancellationToken::new_search();
    let _ = CancellationToken::new_search();
    let outcome = cache
        .search_with_options_base("report.md", Some(&scope), SearchOptions::default(), token)
        .expect("search should not fail");

    assert!(outcome.is_cancelled());
}

#[test]
fn directory_query_with_normalization_fallback_respects_scope_base() {
    let temp_dir = TempDir::new("directory_query_normalization_scope").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Work/Docs")).unwrap();
    fs::create_dir_all(root.join("Personal/Docs")).unwrap();
    fs::write(root.join("Work/Docs/cafe\u{301}.md"), b"work").unwrap();
    fs::write(root.join("Personal/Docs/cafe\u{301}.md"), b"personal").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let nodes = scoped_search(&mut cache, Some("Work/Docs"), Some("café.md"));
    let paths = paths_from_nodes(&nodes);

    assert_eq!(paths.len(), 1);
    assert!(
        paths[0].ends_with("/Work/Docs/café.md") || paths[0].ends_with("/Work/Docs/cafe\u{301}.md")
    );
}

#[test]
fn directory_query_with_base_filter_permutations_stay_inside_scope() {
    let (_temp_dir, _root, mut cache) = build_base_filter_fixture("base_filter_permutations_scope");

    for query in [
        "file: report ext:md size:>3k content:needle dm:today",
        "content:needle size:>3k ext:md file: report dm:today",
        "dm:today report content:needle ext:md size:>3k file:",
    ] {
        let nodes = scoped_search(&mut cache, Some("Work/Docs"), Some(query));
        assert_node_suffixes(&nodes, &["/Work/Docs/report.md"]);
    }
}

#[test]
fn directory_query_with_base_filter_or_and_not_combinations_do_not_escape_scope() {
    let (_temp_dir, _root, mut cache) = build_base_filter_fixture("base_filter_boolean_scope");

    for query in [
        "(ext:md | ext:txt) content:needle !archive",
        "content:needle !archive (ext:md | ext:txt)",
        "!(archive) (ext:md | ext:txt) content:needle",
    ] {
        let nodes = scoped_search(&mut cache, Some("Work/Docs"), Some(query));
        assert_node_suffixes(
            &nodes,
            &[
                "/Work/Docs/report.md",
                "/Work/Docs/notes.txt",
                "/Work/Docs/Nested/nested.md",
            ],
        );
    }
}

#[test]
fn directory_query_with_base_path_filters_compose_with_other_filters() {
    let (_temp_dir, root, mut cache) = build_base_filter_fixture("base_path_filters_scope");
    let work_docs = root.join("Work/Docs");
    let work_nested = root.join("Work/Docs/Nested");
    let personal_docs = root.join("Personal/Docs");
    let personal_nested = root.join("Personal/Docs/Nested");

    for query in [
        format!(
            r#"parent:"{}" ext:md content:needle !archive"#,
            work_docs.display()
        ),
        format!(
            r#"ext:md !archive content:needle parent:"{}""#,
            work_docs.display()
        ),
        format!(
            r#"nosubfolders:"{}" ext:md content:needle !archive"#,
            work_docs.display()
        ),
    ] {
        let nodes = scoped_search(&mut cache, Some("Work/Docs"), Some(&query));
        assert_node_suffixes(&nodes, &["/Work/Docs/report.md"]);
    }

    let nested_query = format!(
        r#"infolder:"{}" ext:md content:needle"#,
        work_nested.display()
    );
    let nodes = scoped_search(&mut cache, Some("Work/Docs"), Some(&nested_query));
    assert_node_suffixes(&nodes, &["/Work/Docs/Nested/nested.md"]);

    let outside_query = format!(
        r#"parent:"{}" | infolder:"{}""#,
        personal_docs.display(),
        personal_nested.display()
    );
    let nodes = scoped_search(&mut cache, Some("Work/Docs"), Some(&outside_query));
    assert!(nodes.is_empty(), "outside path filters leaked {nodes:?}");
}

#[test]
fn directory_query_with_base_type_filters_with_arguments_stay_inside_scope() {
    let (_temp_dir, _root, mut cache) =
        build_base_filter_fixture("base_type_argument_filters_scope");

    for (query, expected) in [
        ("file:report", vec!["/Work/Docs/report.md"]),
        ("folder:FolderMatch", vec!["/Work/Docs/FolderMatch"]),
        ("doc:report", vec!["/Work/Docs/report.md"]),
        ("type:doc report", vec!["/Work/Docs/report.md"]),
        (
            "folder: | file:",
            vec![
                "/Work/Docs/FolderMatch",
                "/Work/Docs/Nested",
                "/Work/Docs/archive.md",
                "/Work/Docs/clip.mp4",
                "/Work/Docs/draft.pdf",
                "/Work/Docs/notes.txt",
                "/Work/Docs/report.md",
                "/Work/Docs/song.mp3",
                "/Work/Docs/tool.exe",
                "/Work/Docs/Nested/nested.md",
            ],
        ),
    ] {
        let nodes = scoped_search(&mut cache, Some("Work/Docs"), Some(query));
        assert_node_suffixes(&nodes, &expected);
    }
}

#[test]
fn directory_query_with_base_media_executable_and_created_filters_stay_inside_scope() {
    let (_temp_dir, _root, mut cache) =
        build_base_filter_fixture("base_media_executable_filters_scope");

    for (query, expected) in [
        ("audio:song dc:today", vec!["/Work/Docs/song.mp3"]),
        ("video:clip dc:today", vec!["/Work/Docs/clip.mp4"]),
        ("exe:tool dc:today", vec!["/Work/Docs/tool.exe"]),
        (
            "(type:audio | type:video | type:exe) content:needle dc:today",
            vec![
                "/Work/Docs/clip.mp4",
                "/Work/Docs/song.mp3",
                "/Work/Docs/tool.exe",
            ],
        ),
    ] {
        let nodes = scoped_search(&mut cache, Some("Work/Docs"), Some(query));
        assert_node_suffixes(&nodes, &expected);
    }
}

#[test]
fn search_with_options_base_filter_branches_all_share_base() {
    let (_temp_dir, root, mut cache) = build_base_filter_fixture("explicit_base_filter_branches");
    let work_docs = root.join("Work/Docs");
    let personal_docs = root.join("Personal/Docs");
    let personal_nested = root.join("Personal/Docs/Nested");
    let scope = guard_indices(cache.search_directory_scope(
        "Work/Docs",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));

    let or_query = format!(
        r#"(parent:"{}" | ext:txt) content:needle"#,
        personal_docs.display()
    );
    let indices = guard_indices(cache.search_with_options_base(
        &or_query,
        Some(&scope),
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    let nodes = cache.expand_file_nodes(&indices);
    assert_node_suffixes(&nodes, &["/Work/Docs/notes.txt"]);

    let mixed_query = format!(
        r#"(infolder:"{}" | parent:"{}") ext:md !archive"#,
        personal_nested.display(),
        work_docs.display()
    );
    let indices = guard_indices(cache.search_with_options_base(
        &mixed_query,
        Some(&scope),
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    let nodes = cache.expand_file_nodes(&indices);
    assert_node_suffixes(&nodes, &["/Work/Docs/report.md"]);

    let not_query = format!(r#"!(parent:"{}") ext:md !archive"#, personal_docs.display());
    let indices = guard_indices(cache.search_with_options_base(
        &not_query,
        Some(&scope),
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    let nodes = cache.expand_file_nodes(&indices);
    assert_node_suffixes(
        &nodes,
        &["/Work/Docs/report.md", "/Work/Docs/Nested/nested.md"],
    );
}

#[test]
fn overlapping_directory_scope_matches_do_not_duplicate_results() {
    let temp_dir = TempDir::new("directory_query_overlapping_scopes").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("dup/dup")).unwrap();
    fs::write(root.join("dup/dup/target.txt"), b"target").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let nodes = scoped_search(&mut cache, Some("dup"), Some("target.txt"));
    let paths = paths_from_nodes(&nodes);

    assert_eq!(paths.len(), 1);
    assert!(paths[0].ends_with("/dup/dup/target.txt"));
}

#[test]
fn directory_query_respects_case_sensitivity_option() {
    let temp_dir = TempDir::new("directory_query_case_sensitivity").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Work/Docs")).unwrap();
    fs::write(root.join("Work/Docs/report.md"), b"report").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let sensitive = scoped_search_with_options(
        &mut cache,
        Some("work/docs"),
        Some("report.md"),
        SearchOptions {
            case_insensitive: false,
        },
        CancellationToken::noop(),
    );
    assert!(sensitive.is_empty());

    let insensitive = scoped_search_with_options(
        &mut cache,
        Some("work/docs"),
        Some("report.md"),
        SearchOptions {
            case_insensitive: true,
        },
        CancellationToken::noop(),
    );
    let paths = paths_from_nodes(&insensitive);

    assert_eq!(paths.len(), 1);
    assert!(paths[0].ends_with("/Work/Docs/report.md"));
}

#[test]
fn directory_query_with_main_query_rejects_file_type_scope() {
    let temp_dir = TempDir::new("directory_query_file_type_scope").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Folder")).unwrap();
    fs::write(root.join("Folder/inside.txt"), b"inside").unwrap();
    fs::write(root.join("OnlyFile"), b"not a directory").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let nodes = scoped_search(&mut cache, Some("file:OnlyFile"), Some("*"));

    assert!(nodes.is_empty());
}

#[test]
fn directory_query_requires_directory_matches() {
    let temp_dir = TempDir::new("directory_query_requires_directory").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Folder")).unwrap();
    fs::write(root.join("Folder/inside.txt"), b"inside").unwrap();
    fs::write(root.join("OnlyFile"), b"not a directory").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let nodes = scoped_search(&mut cache, Some("OnlyFile"), Some("*"));

    assert!(nodes.is_empty());
}

#[test]
fn directory_query_search_propagates_cancelled_token() {
    let temp_dir = TempDir::new("directory_query_cancelled").unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("Folder")).unwrap();
    fs::write(root.join("Folder/inside.txt"), b"inside").unwrap();

    let mut cache = SearchCache::walk_fs(root);
    let token = CancellationToken::new_search();
    let _ = CancellationToken::new_search();
    let outcome = scoped_search_outcome(&mut cache, Some("Folder"), Some("*"), token);

    assert!(outcome.is_cancelled());
}

#[test]
fn test_construct_node_slab_name_index_preserves_path_order() {
    let tree = make_node(
        "root",
        vec![
            make_node("alpha", vec![make_leaf("shared")]),
            make_node("beta", vec![make_node("gamma", vec![make_leaf("shared")])]),
            make_leaf("shared"),
        ],
    );
    let mut slab = ThinSlab::new();
    let mut name_index = NameIndex::default();
    let root = construct_node_slab_name_index(None, &tree, &mut slab, &mut name_index);
    let file_nodes = FileNodes::new(
        PathBuf::from("/virtual/root"),
        Vec::new(),
        Vec::new(),
        slab,
        root,
    );

    let shared_entries = name_index.get("shared").expect("shared entries");
    assert_eq!(shared_entries.len(), 3);
    let paths: Vec<PathBuf> = shared_entries
        .iter()
        .map(|index| file_nodes.node_path(*index).expect("path must exist"))
        .collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(
        paths, sorted,
        "shared entries must follow lexicographic path order"
    );
}

#[test]
fn test_name_index_add_index_sorts_paths() {
    let (file_nodes, targets) = manual_target_tree_file_nodes();
    let mut name_index = NameIndex::default();

    for &index in targets.iter().rev() {
        name_index.add_index("target.txt", index, &file_nodes);
    }

    let entries = name_index
        .get("target.txt")
        .expect("target.txt entries must exist");
    assert_eq!(entries.len(), 3);
    let paths: Vec<PathBuf> = entries
        .iter()
        .map(|index| file_nodes.node_path(*index).expect("path exists"))
        .collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted, "add_index must maintain lexicographic order");
}

#[test]
fn test_walk_fs_with_walk_data_preserves_name_index_order() {
    let temp_dir =
        TempDir::new("walk_fs_with_walk_data_orders").expect("Failed to create temp dir");
    let root = temp_dir.path();
    fs::create_dir(root.join("beta")).unwrap();
    fs::create_dir(root.join("alpha")).unwrap();
    fs::File::create(root.join("target.txt")).unwrap();
    fs::File::create(root.join("alpha/target.txt")).unwrap();
    fs::File::create(root.join("beta/target.txt")).unwrap();

    let walk_data = WalkData::simple(root, false);
    let cache =
        SearchCache::walk_fs_with_walk_data(&walk_data, &NEVER_STOPPED).expect("walk cache");

    let entries = cache
        .name_index
        .get("target.txt")
        .expect("target.txt entries");
    assert_eq!(entries.len(), 3);
    let paths: Vec<PathBuf> = entries
        .iter()
        .map(|index| cache.file_nodes.node_path(*index).expect("path exists"))
        .collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(
        paths, sorted,
        "walk_fs_with_walk_data must yield lexicographically ordered slab indices"
    );
}

#[test]
fn test_search_cache_walk_and_verify() {
    let temp_dir = TempDir::new("test_cache").expect("Failed to create temp directory");
    let temp_path = temp_dir.path();

    fs::create_dir_all(temp_path.join("subdir")).expect("Failed to create subdirectory");
    fs::File::create(temp_path.join("file1.txt")).expect("Failed to create file");
    fs::File::create(temp_path.join("subdir/file2.txt")).expect("Failed to create file");

    let cache = SearchCache::walk_fs(temp_path);

    assert_eq!(cache.file_nodes.len(), 4 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 4 + depth(temp_path));
}

#[test]
fn create_node_chain_existing_path_is_idempotent() {
    let temp_dir = TempDir::new("create_node_chain_existing_path_is_idempotent")
        .expect("Failed to create temp directory");
    let root = temp_dir.path();
    fs::create_dir_all(root.join("alpha/beta")).expect("Failed to create directories");

    let mut cache = SearchCache::walk_fs(root);
    let target = root.join("alpha/beta");
    let before = cache.file_nodes.len();
    let first = cache.create_node_chain(&target);
    let after_first = cache.file_nodes.len();
    let second = cache.create_node_chain(&target);

    assert_eq!(first, second, "existing path should return stable index");
    assert_eq!(before, after_first, "existing path should not add nodes");
    assert_eq!(
        cache.file_nodes.node_path(first).expect("node path exists"),
        target
    );
}

#[test]
fn create_node_chain_creates_missing_tail_with_unaccessible_metadata() {
    let temp_dir = TempDir::new("create_node_chain_creates_missing_tail")
        .expect("Failed to create temp directory");
    let root = temp_dir.path();
    fs::create_dir_all(root.join("existing")).expect("Failed to create directories");

    let mut cache = SearchCache::walk_fs(root);
    let target = root.join("existing/missing/leaf");
    let before = cache.file_nodes.len();
    let index = cache.create_node_chain(&target);
    let after = cache.file_nodes.len();

    assert_eq!(after, before + 2, "missing tail should add two nodes");
    assert_eq!(
        cache.file_nodes.node_path(index).expect("node path exists"),
        target
    );

    let existing_index = find_node_index(&cache, &root.join("existing"));
    let missing_index = find_node_index(&cache, &root.join("existing/missing"));
    let leaf_index = find_node_index(&cache, &target);

    assert_eq!(cache.file_nodes[existing_index].state(), State::Some);
    assert_eq!(cache.file_nodes[missing_index].state(), State::Unaccessible);
    assert_eq!(cache.file_nodes[leaf_index].state(), State::Unaccessible);

    let missing_entries = cache
        .name_index
        .get("missing")
        .expect("missing entry should exist");
    assert!(missing_entries.iter().any(|&idx| idx == missing_index));
    let leaf_entries = cache
        .name_index
        .get("leaf")
        .expect("leaf entry should exist");
    assert!(leaf_entries.iter().any(|&idx| idx == leaf_index));
}

#[test]
fn create_node_chain_existing_file_metadata_is_unchanged() {
    let temp_dir = TempDir::new("create_node_chain_existing_file_metadata")
        .expect("Failed to create temp directory");
    let root = temp_dir.path();
    fs::create_dir_all(root.join("dir")).expect("Failed to create directories");
    fs::File::create(root.join("dir/file.txt")).expect("Failed to create file");

    let mut cache = SearchCache::walk_fs(root);
    let target = root.join("dir/file.txt");
    let index = cache.create_node_chain(&target);

    assert_eq!(cache.file_nodes[index].state(), State::None);
    assert_eq!(cache.file_nodes[index].file_type_hint(), NodeFileType::File);
}

#[test]
fn create_node_chain_root_returns_root() {
    let temp_dir = TempDir::new("create_node_chain_root_returns_root")
        .expect("Failed to create temp directory");
    let root = temp_dir.path();
    let mut cache = SearchCache::walk_fs(root);
    let before = cache.file_nodes.len();

    let index = cache.create_node_chain(Path::new("/"));

    assert_eq!(index, cache.file_nodes.root());
    assert_eq!(cache.file_nodes.len(), before);
}

#[test]
#[should_panic(expected = "create_node_chain only accepts absolute path")]
fn create_node_chain_rejects_relative_paths() {
    let temp_dir = TempDir::new("create_node_chain_rejects_relative_paths")
        .expect("Failed to create temp directory");
    let root = temp_dir.path();
    let mut cache = SearchCache::walk_fs(root);

    let _ = cache.create_node_chain(Path::new("relative/path"));
}

// --- New comprehensive tests for recent changes ---

#[test]
fn node_index_for_path_with_absolute_paths() {
    let temp_dir =
        TempDir::new("node_index_for_path_absolute").expect("Failed to create temp directory");
    let root = temp_dir.path();
    fs::create_dir_all(root.join("alpha/beta/gamma")).expect("Failed to create directories");
    fs::File::create(root.join("alpha/beta/file.txt")).expect("Failed to create file");

    let cache = SearchCache::walk_fs(root);

    // Test retrieval with absolute paths
    let alpha_index = cache.node_index_for_path(&root.join("alpha"));
    assert!(alpha_index.is_some(), "should find alpha directory");

    let beta_index = cache.node_index_for_path(&root.join("alpha/beta"));
    assert!(beta_index.is_some(), "should find alpha/beta directory");

    let file_index = cache.node_index_for_path(&root.join("alpha/beta/file.txt"));
    assert!(file_index.is_some(), "should find file.txt");

    let nonexistent = cache.node_index_for_path(&root.join("alpha/nonexistent"));
    assert!(nonexistent.is_none(), "should not find nonexistent path");
}

#[test]
fn node_index_for_path_with_relative_path_fails() {
    let temp_dir =
        TempDir::new("node_index_for_path_relative").expect("Failed to create temp directory");
    let root = temp_dir.path();
    fs::create_dir_all(root.join("dir")).expect("Failed to create directory");

    let cache = SearchCache::walk_fs(root);

    // Relative paths should not be found
    let result = cache.node_index_for_path(Path::new("dir"));
    assert!(result.is_none(), "relative paths should not match");
}

#[test]
fn node_path_returns_absolute_paths() {
    let temp_dir = TempDir::new("node_path_absolute").expect("Failed to create temp directory");
    let root = temp_dir.path();
    fs::create_dir_all(root.join("folder/subfolder")).expect("Failed to create directories");
    fs::File::create(root.join("folder/file.txt")).expect("Failed to create file");

    let cache = SearchCache::walk_fs(root);

    let folder_index = cache
        .node_index_for_path(&root.join("folder"))
        .expect("folder should exist");
    let folder_path = cache
        .node_path(folder_index)
        .expect("should get folder path");
    assert!(
        folder_path.is_absolute(),
        "returned path should be absolute"
    );
    assert_eq!(folder_path, root.join("folder"));

    let file_index = cache
        .node_index_for_path(&root.join("folder/file.txt"))
        .expect("file should exist");
    let file_path = cache.node_path(file_index).expect("should get file path");
    assert!(file_path.is_absolute(), "returned path should be absolute");
    assert_eq!(file_path, root.join("folder/file.txt"));
}

#[test]
fn walk_it_creates_full_parent_chain() {
    let temp_dir = TempDir::new("walk_it_parent_chain").expect("Failed to create temp directory");
    let root = temp_dir.path();
    fs::create_dir_all(root.join("deep/nested/structure")).expect("Failed to create directories");

    let cache = SearchCache::walk_fs(root);

    // Verify root node exists at the filesystem root
    let root_index = cache.file_nodes.root();
    let root_name = cache.file_nodes[root_index].name();
    assert_eq!(root_name, "/", "root node should be named '/'");

    // Traverse down to verify the chain is complete
    let mut current_index = root_index;
    for segment in root.strip_prefix("/").unwrap() {
        let found = cache.file_nodes[current_index]
            .children
            .iter()
            .find(|&&child_idx| cache.file_nodes[child_idx].name() == segment);
        assert!(found.is_some(), "should find segment in parent chain",);
        current_index = *found.unwrap();
    }
}

#[test]
fn walk_it_deep_path_has_all_ancestors() {
    let temp_dir = TempDir::new("walk_it_deep_ancestors").expect("Failed to create temp directory");
    let root = temp_dir.path();
    fs::create_dir_all(root.join("a/b/c/d/e")).expect("Failed to create directories");

    let cache = SearchCache::walk_fs(root);

    // Find the deepest node
    let e_index = cache
        .node_index_for_path(&root.join("a/b/c/d/e"))
        .expect("deepest node should exist");

    // Verify we can navigate all the way back to root
    let mut current = e_index;
    let mut visited = vec![];

    // Traverse upwards to collect all ancestors
    loop {
        visited.push(current);
        if current == cache.file_nodes.root() {
            break;
        }
        let parent = cache.file_nodes[current].parent();
        if let Some(p) = parent {
            current = p;
        } else {
            panic!("Node should have parent until reaching root");
        }
    }

    // Should have at least: /, <temp_dir_segments...>, a, b, c, d, e
    assert!(
        visited.len() >= 6,
        "should have visited at least 6 ancestors (including self)"
    );
}

#[test]
fn remove_node_path_with_absolute_paths() {
    let temp_dir = TempDir::new("remove_node_absolute").expect("Failed to create temp directory");
    let root = temp_dir.path();
    fs::create_dir_all(root.join("to_remove/child")).expect("Failed to create directories");
    fs::File::create(root.join("to_remove/file.txt")).expect("Failed to create file");

    let mut cache = SearchCache::walk_fs(root);

    let before_count = cache.file_nodes.len();
    let target_path = root.join("to_remove");

    // Verify node exists before removal
    assert!(cache.node_index_for_path(&target_path).is_some());

    // Remove the node
    let removed = cache.remove_node_path(&target_path);
    assert!(removed.is_some(), "should return removed node index");

    let after_count = cache.file_nodes.len();
    assert!(
        after_count < before_count,
        "node count should decrease after removal"
    );

    // Verify node no longer exists
    assert!(cache.node_index_for_path(&target_path).is_none());
    assert!(
        cache
            .node_index_for_path(&target_path.join("child"))
            .is_none()
    );
}

#[test]
fn create_node_chain_with_deep_missing_ancestors() {
    let temp_dir =
        TempDir::new("create_node_chain_deep_missing").expect("Failed to create temp directory");
    let root = temp_dir.path();
    fs::create_dir_all(root.join("exists")).expect("Failed to create directory");

    let mut cache = SearchCache::walk_fs(root);
    let before = cache.file_nodes.len();

    // Create chain for a deeply nested path that doesn't exist on disk
    let target = root.join("exists/missing1/missing2/missing3/leaf");
    let index = cache.create_node_chain(&target);
    let after = cache.file_nodes.len();

    // Should add 4 new nodes: missing1, missing2, missing3, leaf
    assert_eq!(after, before + 4, "should add 4 missing nodes");

    // Verify path is correct
    assert_eq!(
        cache.node_path(index).expect("node path should exist"),
        target
    );

    // Verify all intermediate nodes are marked as unaccessible
    let missing1_index = cache
        .node_index_for_path(&root.join("exists/missing1"))
        .expect("missing1 should exist");
    assert_eq!(
        cache.file_nodes[missing1_index].state(),
        State::Unaccessible
    );
}

#[test]
fn create_node_chain_preserves_existing_metadata() {
    let temp_dir = TempDir::new("create_node_chain_preserves_metadata")
        .expect("Failed to create temp directory");
    let root = temp_dir.path();
    fs::create_dir_all(root.join("dir")).expect("Failed to create directory");
    fs::File::create(root.join("dir/existing.txt")).expect("Failed to create file");

    let mut cache = SearchCache::walk_fs(root);

    // Get the original metadata state
    let existing_index = cache
        .node_index_for_path(&root.join("dir/existing.txt"))
        .expect("file should exist");
    let original_state = cache.file_nodes[existing_index].state();
    let original_type = cache.file_nodes[existing_index].file_type_hint();

    // Call create_node_chain on the existing path
    let new_index = cache.create_node_chain(&root.join("dir/existing.txt"));

    // Should return the same index
    assert_eq!(new_index, existing_index);

    // Metadata should be unchanged
    assert_eq!(cache.file_nodes[new_index].state(), original_state);
    assert_eq!(cache.file_nodes[new_index].file_type_hint(), original_type);
}

#[test]
fn scan_path_recursive_with_absolute_path() {
    let temp_dir =
        TempDir::new("scan_path_recursive_absolute").expect("Failed to create temp directory");
    let root = temp_dir.path();

    let mut cache = SearchCache::walk_fs(root);
    let initial_count = cache.file_nodes.len();

    // Create new content after initial walk
    fs::create_dir_all(root.join("new_dir/sub")).expect("Failed to create directories");
    fs::File::create(root.join("new_dir/file.txt")).expect("Failed to create file");

    // Scan the new path with absolute path
    let result = cache.scan_path_recursive(&root.join("new_dir"));
    assert!(result.is_some(), "should successfully scan new directory");

    let after_count = cache.file_nodes.len();
    assert!(
        after_count > initial_count,
        "should have added nodes from scan"
    );

    // Verify the new nodes are accessible
    assert!(cache.node_index_for_path(&root.join("new_dir")).is_some());
    assert!(
        cache
            .node_index_for_path(&root.join("new_dir/sub"))
            .is_some()
    );
    assert!(
        cache
            .node_index_for_path(&root.join("new_dir/file.txt"))
            .is_some()
    );
}

#[test]
fn scan_path_recursive_handles_nonexistent_file() {
    let temp_dir = TempDir::new("scan_path_nonexistent").expect("Failed to create temp directory");
    let root = temp_dir.path();
    fs::create_dir_all(root.join("existing")).expect("Failed to create directory");

    let mut cache = SearchCache::walk_fs(root);

    // Try to scan a path that doesn't exist
    let result = cache.scan_path_recursive(&root.join("existing/nonexistent.txt"));
    assert!(result.is_none(), "should return None for nonexistent path");
}

#[test]
fn path_handling_with_unicode_characters() {
    let temp_dir = TempDir::new("path_unicode").expect("Failed to create temp directory");
    let root = temp_dir.path();
    fs::create_dir_all(root.join("文件夹/子目录")).expect("Failed to create directories");
    fs::File::create(root.join("文件夹/文件.txt")).expect("Failed to create file");

    let cache = SearchCache::walk_fs(root);

    // Verify Unicode paths work correctly
    let folder_index = cache.node_index_for_path(&root.join("文件夹"));
    assert!(folder_index.is_some(), "should find Unicode folder");

    let file_index = cache.node_index_for_path(&root.join("文件夹/文件.txt"));
    assert!(file_index.is_some(), "should find Unicode file");

    // Verify path reconstruction
    let file_path = cache
        .node_path(file_index.unwrap())
        .expect("should get file path");
    assert_eq!(file_path, root.join("文件夹/文件.txt"));
}

#[test]
fn path_handling_with_special_characters() {
    let temp_dir = TempDir::new("path_special").expect("Failed to create temp directory");
    let root = temp_dir.path();
    fs::create_dir_all(root.join("dir with spaces/sub-dir")).expect("Failed to create directories");
    fs::File::create(root.join("dir with spaces/file (1).txt")).expect("Failed to create file");

    let cache = SearchCache::walk_fs(root);

    // Verify paths with spaces and special chars work
    let dir_index = cache.node_index_for_path(&root.join("dir with spaces"));
    assert!(dir_index.is_some(), "should find directory with spaces");

    let file_index = cache.node_index_for_path(&root.join("dir with spaces/file (1).txt"));
    assert!(
        file_index.is_some(),
        "should find file with special characters"
    );
}

#[test]
fn walk_it_without_root_chain_comparison() {
    let temp_dir =
        TempDir::new("walk_without_root_chain").expect("Failed to create temp directory");
    let root = temp_dir.path();
    fs::create_dir_all(root.join("test_dir")).expect("Failed to create directory");

    let walk_data = WalkData::simple(root, true);

    // Test walk_it_without_root_chain
    let tree_without_chain = walk_it_without_root_chain(&walk_data).expect("walk succeeded");
    assert_eq!(
        &*tree_without_chain.name,
        root.file_name().unwrap().to_str().unwrap(),
        "root node should match directory name"
    );

    // Test walk_it (with root chain)
    let tree_with_chain = walk_it(&walk_data).expect("walk succeeded");
    assert_eq!(
        &*tree_with_chain.name, "/",
        "root of chain should be filesystem root"
    );

    // Navigate down the chain to find the actual root directory
    let root_name = root.file_name().unwrap().to_str().unwrap();

    // Traverse to find the target directory in the chain
    let mut found_root = false;
    fn find_in_tree<'a>(node: &'a Node, name: &str) -> Option<&'a Node> {
        if &*node.name == name {
            return Some(node);
        }
        for child in &node.children {
            if let Some(found) = find_in_tree(child, name) {
                return Some(found);
            }
        }
        None
    }

    if let Some(target_node) = find_in_tree(&tree_with_chain, root_name) {
        found_root = true;
        // This node should have same structure as tree_without_chain
        assert_eq!(&*target_node.name, &*tree_without_chain.name);
    }

    assert!(found_root, "should find target directory in parent chain");
}

#[test]
fn node_path_edge_cases() {
    let temp_dir = TempDir::new("node_path_edge").expect("Failed to create temp directory");
    let root = temp_dir.path();

    let cache = SearchCache::walk_fs(root);

    // Test root node path
    let root_index = cache.file_nodes.root();
    let root_path = cache.node_path(root_index);
    assert!(root_path.is_some(), "root should have a path");
    assert_eq!(
        root_path.unwrap(),
        PathBuf::from("/"),
        "root path should be '/'"
    );
}

#[test]
fn create_node_chain_intermediate_nodes_have_correct_parents() {
    let temp_dir =
        TempDir::new("create_node_chain_parents").expect("Failed to create temp directory");
    let root = temp_dir.path();
    fs::create_dir_all(root.join("a")).expect("Failed to create directory");

    let mut cache = SearchCache::walk_fs(root);

    // Create a chain with multiple missing levels
    let target = root.join("a/b/c/d");
    cache.create_node_chain(&target);

    // Verify parent relationships
    let d_index = cache.node_index_for_path(&target).expect("d should exist");
    let c_index = cache
        .node_index_for_path(&root.join("a/b/c"))
        .expect("c should exist");
    let b_index = cache
        .node_index_for_path(&root.join("a/b"))
        .expect("b should exist");

    // Check parent pointers
    assert_eq!(
        cache.file_nodes[d_index].parent(),
        Some(c_index),
        "d's parent should be c"
    );
    assert_eq!(
        cache.file_nodes[c_index].parent(),
        Some(b_index),
        "c's parent should be b"
    );
}

#[test]
fn test_handle_fs_event_add() {
    // Create a temporary directory.
    let temp_dir = TempDir::new("test_events").expect("Failed to create temp directory");
    let temp_path = temp_dir.path();

    let mut cache = SearchCache::walk_fs(temp_dir.path());

    assert_eq!(cache.file_nodes.len(), 1 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 1 + depth(temp_path));

    fs::File::create(temp_path.join("new_file.txt")).expect("Failed to create file");

    let mock_events = vec![FsEvent {
        path: temp_path.join("new_file.txt"),
        id: cache.last_event_id + 1,
        flag: EventFlag::ItemCreated,
    }];

    cache.handle_fs_events(mock_events).unwrap();

    assert_eq!(cache.file_nodes.len(), 2 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 2 + depth(temp_path));
    assert_eq!(cache.search("new_file.txt").unwrap().len(), 1);
}

#[test]
fn test_handle_fs_event_add_before_search() {
    let temp_dir = TempDir::new("test_events").expect("Failed to create temp directory");
    let temp_path = temp_dir.path();
    fs::File::create(temp_path.join("new_file.txt")).expect("Failed to create file");

    let mut cache = SearchCache::walk_fs(temp_dir.path());

    assert_eq!(cache.file_nodes.len(), 2 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 2 + depth(temp_path));

    let mock_events = vec![FsEvent {
        path: temp_path.join("new_file.txt"),
        id: cache.last_event_id + 1,
        flag: EventFlag::ItemCreated,
    }];

    cache.handle_fs_events(mock_events).unwrap();

    assert_eq!(cache.file_nodes.len(), 2 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 2 + depth(temp_path));
    assert_eq!(cache.search("new_file.txt").unwrap().len(), 1);
}

// Processing outdated fs event is required to avoid bouncing.
#[test]
fn test_handle_outdated_fs_event() {
    let temp_dir = TempDir::new("test_events").expect("Failed to create temp directory");
    let temp_path = temp_dir.path();

    let mut cache = SearchCache::walk_fs(temp_dir.path());

    assert_eq!(cache.file_nodes.len(), 1 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 1 + depth(temp_path));

    fs::File::create(temp_path.join("new_file.txt")).expect("Failed to create file");

    let mock_events = vec![FsEvent {
        path: temp_path.join("new_file.txt"),
        id: cache.last_event_id.saturating_sub(1),
        flag: EventFlag::ItemCreated,
    }];

    cache.handle_fs_events(mock_events).unwrap();

    assert_eq!(cache.file_nodes.len(), 2 + depth(temp_path));
    assert_eq!(cache.name_index.len(), 2 + depth(temp_path));
    assert_eq!(cache.search("new_file.txt").unwrap().len(), 1);
}

#[test]
fn test_search_with_regex_query() {
    let temp_dir = TempDir::new("test_search_regex_query").unwrap();
    let dir = temp_dir.path();

    fs::File::create(dir.join("foo123.txt")).unwrap();
    fs::File::create(dir.join("bar.txt")).unwrap();

    let mut cache = SearchCache::walk_fs(dir);
    let indices = cache.search("regex:foo\\d+").unwrap();
    assert_eq!(indices.len(), 1);
    let nodes = cache.expand_file_nodes(&indices);
    assert_eq!(nodes.len(), 1);
    assert!(nodes[0].path.ends_with("foo123.txt"));

    // ensure other names are not matched
    let miss = cache.search("regex:bar\\d+").unwrap();
    assert!(miss.is_empty());
}

#[test]
fn and_with_not_propagates_cancellation() {
    let temp_dir = TempDir::new("and_with_not_propagates_cancellation").unwrap();
    let dir = temp_dir.path();

    fs::File::create(dir.join("foo.txt")).unwrap();
    fs::File::create(dir.join("bar.txt")).unwrap();

    let mut cache = SearchCache::walk_fs(dir);
    let token = CancellationToken::new_search();
    let _ = CancellationToken::new_search(); // cancel previous token

    let result = cache.search_with_options(
        "bar !foo",
        SearchOptions {
            case_insensitive: false,
        },
        token,
    );
    assert!(matches!(result, Ok(SearchOutcome { nodes: None, .. })));
}

#[test]
fn test_search_case_insensitive_option() {
    let temp_dir = TempDir::new("test_search_case_insensitive_option").unwrap();
    let dir = temp_dir.path();

    fs::File::create(dir.join("Alpha.TXT")).unwrap();
    fs::File::create(dir.join("beta.txt")).unwrap();

    let mut cache = SearchCache::walk_fs(dir);
    let opts = SearchOptions {
        case_insensitive: true,
    };
    let indices =
        guard_indices(cache.search_with_options("alpha.txt", opts, CancellationToken::noop()));
    assert_eq!(indices.len(), 1);
    let nodes = cache.expand_file_nodes(&indices);
    assert_eq!(nodes.len(), 1);
    assert!(nodes[0].path.ends_with("Alpha.TXT"));

    let opts = SearchOptions {
        case_insensitive: true,
    };
    let miss =
        guard_indices(cache.search_with_options("gamma.txt", opts, CancellationToken::noop()));
    assert!(miss.is_empty());
}
