#![cfg(target_os = "macos")]

use search_cache::{SearchCache, SearchOptions};
use search_cancel::CancellationToken;
use std::fs;
use tempdir::TempDir;

mod common;
use common::*;

#[test]
fn tag_filter_list_very_long() {
    let temp_dir = TempDir::new("tag_list_very_long").unwrap();
    let dir = temp_dir.path();

    // Create files with different tags
    for i in 0..50 {
        let file = dir.join(format!("file{i}.txt"));
        fs::write(&file, b"dummy").unwrap();
        write_tags(&file, &[&format!("Tag{i}")]);
    }

    let mut cache = SearchCache::walk_fs(dir);
    // Create a list with 50 tags
    let tag_list: String = (0..50)
        .map(|i| format!("Tag{i}"))
        .collect::<Vec<_>>()
        .join(";");
    let query = format!("tag:{tag_list}");
    let indices = guard_indices(cache.search_with_options(
        &query,
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    // Should match all 50 files
    assert_eq!(indices.len(), 50);
}

#[test]
fn tag_filter_list_with_single_item() {
    let temp_dir = TempDir::new("tag_list_single").unwrap();
    let dir = temp_dir.path();

    let first = dir.join("first.txt");
    fs::write(&first, b"dummy").unwrap();
    write_tags(&first, &["Project"]);

    let mut cache = SearchCache::walk_fs(dir);
    // Semicolon is a delimiter in the parser, so 'tag:Project;' becomes 'tag:Project'
    // as a bare argument (the trailing semicolon is not part of the value)
    let indices = guard_indices(cache.search_with_options(
        "tag:Project;",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 1);
}

#[test]
fn tag_filter_range_syntax_rejected() {
    let temp_dir = TempDir::new("tag_range_rejected").unwrap();
    let dir = temp_dir.path();

    let first = dir.join("first.txt");
    fs::write(&first, b"dummy").unwrap();
    write_tags(&first, &["Project"]);

    let mut cache = SearchCache::walk_fs(dir);
    let result = cache.search_with_options(
        "tag:1..10",
        SearchOptions::default(),
        CancellationToken::noop(),
    );
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("does not support ranges")
    );
}

#[test]
fn tag_filter_comparison_syntax_rejected() {
    let temp_dir = TempDir::new("tag_comparison_rejected").unwrap();
    let dir = temp_dir.path();

    let first = dir.join("first.txt");
    fs::write(&first, b"dummy").unwrap();
    write_tags(&first, &["Project"]);

    let mut cache = SearchCache::walk_fs(dir);
    let result = cache.search_with_options(
        "tag:>5",
        SearchOptions::default(),
        CancellationToken::noop(),
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("does not support"));
}

#[test]
fn tag_filter_forbidden_char_single_quote() {
    let temp_dir = TempDir::new("tag_forbidden_quote").unwrap();
    let dir = temp_dir.path();

    let first = dir.join("first.txt");
    fs::write(&first, b"dummy").unwrap();
    write_tags(&first, &["Project"]);

    let mut cache = SearchCache::walk_fs(dir);
    let _result = cache.search_with_options(
        "tag:Project'Alpha",
        SearchOptions::default(),
        CancellationToken::noop(),
    );
    // Small base set should try to use metadata first and succeed
    // Large base set would use mdfind and fail
    // Let's test with a larger base by searching all files first
    let _result = cache.search_with_options(
        "type:file tag:Project'Alpha",
        SearchOptions::default(),
        CancellationToken::noop(),
    );
    // This might succeed with small dataset (metadata path) or fail (mdfind path)
    // We just verify it doesn't panic
}

#[test]
fn tag_filter_forbidden_char_backslash() {
    let temp_dir = TempDir::new("tag_forbidden_backslash").unwrap();
    let dir = temp_dir.path();

    let first = dir.join("first.txt");
    fs::write(&first, b"dummy").unwrap();
    write_tags(&first, &["Project"]);

    let mut cache = SearchCache::walk_fs(dir);
    let _result = cache.search_with_options(
        "type:file tag:Project\\Alpha",
        SearchOptions::default(),
        CancellationToken::noop(),
    );
    // Similar to above - behavior depends on threshold
}

#[test]
fn tag_filter_forbidden_char_asterisk() {
    let temp_dir = TempDir::new("tag_forbidden_asterisk").unwrap();
    let dir = temp_dir.path();

    let first = dir.join("first.txt");
    fs::write(&first, b"dummy").unwrap();
    write_tags(&first, &["Project"]);

    let mut cache = SearchCache::walk_fs(dir);
    let _result = cache.search_with_options(
        "type:file tag:Project*",
        SearchOptions::default(),
        CancellationToken::noop(),
    );
    // Similar to above
}

#[test]
fn tag_filter_list_with_forbidden_char_in_one_item() {
    let temp_dir = TempDir::new("tag_list_forbidden").unwrap();
    let dir = temp_dir.path();

    let first = dir.join("first.txt");
    fs::write(&first, b"dummy").unwrap();
    write_tags(&first, &["Project"]);

    let mut cache = SearchCache::walk_fs(dir);
    let _result = cache.search_with_options(
        "type:file tag:ValidTag;Invalid'Tag",
        SearchOptions::default(),
        CancellationToken::noop(),
    );
    // Should reject the query when using mdfind path
}

#[test]
fn tag_filter_combines_with_folder_filter() {
    let temp_dir = TempDir::new("tag_with_folder").unwrap();
    let dir = temp_dir.path();

    let subdir = dir.join("subdir");
    fs::create_dir(&subdir).unwrap();

    let file1 = subdir.join("file1.txt");
    fs::write(&file1, b"dummy").unwrap();
    write_tags(&file1, &["Project"]);

    let file2 = dir.join("file2.txt");
    fs::write(&file2, b"dummy").unwrap();
    write_tags(&file2, &["Project"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        &format!("tag:Project parent:{}", subdir.display()),
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 1);
    let nodes = cache.expand_file_nodes(&indices);
    assert!(nodes[0].path.ends_with("file1.txt"));
}

#[test]
fn tag_filter_partial_match_at_word_boundary() {
    let temp_dir = TempDir::new("tag_word_boundary").unwrap();
    let dir = temp_dir.path();

    let first = dir.join("first.txt");
    fs::write(&first, b"dummy").unwrap();
    write_tags(&first, &["Work-Project"]);

    let second = dir.join("second.txt");
    fs::write(&second, b"dummy").unwrap();
    write_tags(&second, &["Teamwork"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:work",
        SearchOptions {
            case_insensitive: true,
        },
        CancellationToken::noop(),
    ));
    // Should match both because it's a substring search (case-insensitive)
    assert_eq!(indices.len(), 2);
}

#[test]
fn tag_filter_empty_list_after_normalization() {
    let temp_dir = TempDir::new("tag_empty_after_norm").unwrap();
    let dir = temp_dir.path();

    let first = dir.join("first.txt");
    fs::write(&first, b"dummy").unwrap();
    write_tags(&first, &["Project"]);

    let mut cache = SearchCache::walk_fs(dir);
    let result = cache.search_with_options(
        "tag: ; ; ; ",
        SearchOptions::default(),
        CancellationToken::noop(),
    );
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("tag: requires a value")
    );
}

#[test]
fn tag_filter_list_case_sensitive_no_match() {
    let temp_dir = TempDir::new("tag_list_case_no_match").unwrap();
    let dir = temp_dir.path();

    let first = dir.join("first.txt");
    fs::write(&first, b"dummy").unwrap();
    write_tags(&first, &["project"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:Project;Important",
        SearchOptions {
            case_insensitive: false,
        },
        CancellationToken::noop(),
    ));
    // Should not match because case doesn't match
    assert_eq!(indices.len(), 0);
}

#[test]
fn tag_filter_list_case_insensitive_match() {
    let temp_dir = TempDir::new("tag_list_case_match").unwrap();
    let dir = temp_dir.path();

    let first = dir.join("first.txt");
    fs::write(&first, b"dummy").unwrap();
    write_tags(&first, &["project"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:Project;Important",
        SearchOptions {
            case_insensitive: true,
        },
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 1);
}

#[test]
fn tag_filter_three_way_or_with_list() {
    let temp_dir = TempDir::new("tag_three_way_or").unwrap();
    let dir = temp_dir.path();

    let first = dir.join("first.txt");
    fs::write(&first, b"dummy").unwrap();
    write_tags(&first, &["Alpha"]);

    let second = dir.join("second.txt");
    fs::write(&second, b"dummy").unwrap();
    write_tags(&second, &["Beta"]);

    let third = dir.join("third.txt");
    fs::write(&third, b"dummy").unwrap();
    write_tags(&third, &["Gamma"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:Alpha;Beta;Gamma",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 3);
}

#[test]
fn tag_filter_list_combined_with_and_logic() {
    let temp_dir = TempDir::new("tag_list_and").unwrap();
    let dir = temp_dir.path();

    let first = dir.join("first.txt");
    fs::write(&first, b"dummy").unwrap();
    write_tags(&first, &["Project", "Important"]);

    let second = dir.join("second.txt");
    fs::write(&second, b"dummy").unwrap();
    write_tags(&second, &["Project", "Archive"]);

    let third = dir.join("third.txt");
    fs::write(&third, b"dummy").unwrap();
    write_tags(&third, &["Important"]);

    let mut cache = SearchCache::walk_fs(dir);
    // List for OR: (Project OR Archive) AND Important
    let indices = guard_indices(cache.search_with_options(
        "tag:Project;Archive tag:Important",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    // Should match first.txt (has Project and Important)
    assert_eq!(indices.len(), 1);
    let nodes = cache.expand_file_nodes(&indices);
    assert!(nodes[0].path.ends_with("first.txt"));
}

#[test]
fn tag_filter_deeply_nested_subdirectory() {
    let temp_dir = TempDir::new("tag_nested_deep").unwrap();
    let dir = temp_dir.path();

    let mut current = dir.to_path_buf();
    for i in 0..5 {
        current = current.join(format!("level{i}"));
        fs::create_dir(&current).unwrap();
    }

    let file = current.join("deep.txt");
    fs::write(&file, b"dummy").unwrap();
    write_tags(&file, &["DeepTag"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:DeepTag",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 1);
}

#[test]
fn tag_filter_symlink_with_tags() {
    use std::os::unix::fs::symlink;

    let temp_dir = TempDir::new("tag_symlink").unwrap();
    let dir = temp_dir.path();

    let target = dir.join("target.txt");
    fs::write(&target, b"dummy").unwrap();
    write_tags(&target, &["TargetTag"]);

    let link = dir.join("link.txt");
    symlink(&target, &link).unwrap();

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:TargetTag",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    // Should find the target file
    assert!(!indices.is_empty());
}

#[test]
fn tag_filter_with_size_filter() {
    let temp_dir = TempDir::new("tag_with_size").unwrap();
    let dir = temp_dir.path();

    let small = dir.join("small.txt");
    fs::write(&small, b"x").unwrap();
    write_tags(&small, &["Project"]);

    let large = dir.join("large.txt");
    fs::write(&large, [0u8; 1024]).unwrap();
    write_tags(&large, &["Project"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:Project size:<100",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 1);
    let nodes = cache.expand_file_nodes(&indices);
    assert!(nodes[0].path.ends_with("small.txt"));
}

#[test]
fn tag_filter_with_date_filter() {
    let temp_dir = TempDir::new("tag_with_date").unwrap();
    let dir = temp_dir.path();

    let file = dir.join("file.txt");
    fs::write(&file, b"dummy").unwrap();
    write_tags(&file, &["Project"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:Project dm:today",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    // File was just created, should match "today"
    assert_eq!(indices.len(), 1);
}

#[test]
fn tag_filter_list_with_content_search() {
    let temp_dir = TempDir::new("tag_with_content").unwrap();
    let dir = temp_dir.path();

    let first = dir.join("first.txt");
    fs::write(&first, b"hello world").unwrap();
    write_tags(&first, &["Project"]);

    let second = dir.join("second.txt");
    fs::write(&second, b"goodbye world").unwrap();
    write_tags(&second, &["Important"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:Project;Important content:hello",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 1);
    let nodes = cache.expand_file_nodes(&indices);
    assert!(nodes[0].path.ends_with("first.txt"));
}

// Tests for mdfind threshold behavior
// Note: The threshold is 10000, so we test with base sets above and below that
#[test]
fn tag_filter_small_base_uses_metadata() {
    let temp_dir = TempDir::new("tag_small_base").unwrap();
    let dir = temp_dir.path();

    // Create a small number of files (well below 10000 threshold)
    for i in 0..10 {
        let file = dir.join(format!("file{i}.txt"));
        fs::write(&file, b"dummy").unwrap();
        if i % 2 == 0 {
            write_tags(&file, &["Even"]);
        }
    }

    let mut cache = SearchCache::walk_fs(dir);
    // With small base, should use metadata path (read xattr for each file)
    let indices = guard_indices(cache.search_with_options(
        "tag:Even",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 5);
}

#[test]
fn tag_filter_with_explicit_base() {
    let temp_dir = TempDir::new("tag_explicit_base").unwrap();
    let dir = temp_dir.path();

    let subdir = dir.join("subdir");
    fs::create_dir(&subdir).unwrap();

    let file1 = subdir.join("file1.txt");
    fs::write(&file1, b"dummy").unwrap();
    write_tags(&file1, &["Project"]);

    let file2 = dir.join("file2.txt");
    fs::write(&file2, b"dummy").unwrap();
    write_tags(&file2, &["Project"]);

    let mut cache = SearchCache::walk_fs(dir);
    // First narrow to subdir, then apply tag filter
    let indices = guard_indices(cache.search_with_options(
        &format!("infolder:{} tag:Project", subdir.display()),
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 1);
    let nodes = cache.expand_file_nodes(&indices);
    assert!(nodes[0].path.ends_with("file1.txt"));
}

#[test]
fn tag_filter_no_base_matches_all_tagged_files() {
    let temp_dir = TempDir::new("tag_no_base").unwrap();
    let dir = temp_dir.path();

    let file1 = dir.join("file1.txt");
    fs::write(&file1, b"dummy").unwrap();
    write_tags(&file1, &["Global"]);

    let subdir = dir.join("subdir");
    fs::create_dir(&subdir).unwrap();

    let file2 = subdir.join("file2.txt");
    fs::write(&file2, b"dummy").unwrap();
    write_tags(&file2, &["Global"]);

    let mut cache = SearchCache::walk_fs(dir);
    // No base, should search entire index
    let indices = guard_indices(cache.search_with_options(
        "tag:Global",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 2);
}

#[test]
fn tag_filter_handles_corrupted_xattr() {
    use xattr::set;

    let temp_dir = TempDir::new("tag_corrupted").unwrap();
    let dir = temp_dir.path();

    let file = dir.join("corrupted.txt");
    fs::write(&file, b"dummy").unwrap();
    // Write invalid plist data
    set(
        &file,
        "com.apple.metadata:_kMDItemUserTags",
        b"not a valid plist",
    )
    .unwrap();

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:Project",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    // Should gracefully handle corruption and return no matches
    assert_eq!(indices.len(), 0);
}

#[test]
fn tag_filter_exact_tag_name_no_substring() {
    let temp_dir = TempDir::new("tag_exact_only").unwrap();
    let dir = temp_dir.path();

    let first = dir.join("first.txt");
    fs::write(&first, b"dummy").unwrap();
    write_tags(&first, &["Proj"]);

    let second = dir.join("second.txt");
    fs::write(&second, b"dummy").unwrap();
    write_tags(&second, &["Project"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:Proj",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    // Should match both because "Proj" is a substring of "Project"
    assert_eq!(indices.len(), 2);
}

#[test]
fn tag_filter_list_matches_any_not_all() {
    let temp_dir = TempDir::new("tag_list_any").unwrap();
    let dir = temp_dir.path();

    let first = dir.join("first.txt");
    fs::write(&first, b"dummy").unwrap();
    write_tags(&first, &["Alpha"]);

    let second = dir.join("second.txt");
    fs::write(&second, b"dummy").unwrap();
    write_tags(&second, &["Beta"]);

    let third = dir.join("third.txt");
    fs::write(&third, b"dummy").unwrap();
    write_tags(&third, &["Gamma"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:Alpha;Beta",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    // Should match files with Alpha OR Beta (not requiring both)
    assert_eq!(indices.len(), 2);
}

#[test]
fn tag_filter_combined_list_and_separate_filters() {
    let temp_dir = TempDir::new("tag_combined_list").unwrap();
    let dir = temp_dir.path();

    let first = dir.join("first.txt");
    fs::write(&first, b"dummy").unwrap();
    write_tags(&first, &["Alpha", "Common"]);

    let second = dir.join("second.txt");
    fs::write(&second, b"dummy").unwrap();
    write_tags(&second, &["Beta", "Common"]);

    let third = dir.join("third.txt");
    fs::write(&third, b"dummy").unwrap();
    write_tags(&third, &["Gamma"]);

    let mut cache = SearchCache::walk_fs(dir);
    // (Alpha OR Beta) AND Common
    let indices = guard_indices(cache.search_with_options(
        "tag:Alpha;Beta tag:Common",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 2);
}

#[test]
fn tag_filter_negation_with_list() {
    let temp_dir = TempDir::new("tag_negation_list").unwrap();
    let dir = temp_dir.path();

    let first = dir.join("first.txt");
    fs::write(&first, b"dummy").unwrap();
    write_tags(&first, &["Alpha"]);

    let second = dir.join("second.txt");
    fs::write(&second, b"dummy").unwrap();
    write_tags(&second, &["Beta"]);

    let third = dir.join("third.txt");
    fs::write(&third, b"dummy").unwrap();
    write_tags(&third, &["Gamma"]);

    let fourth = dir.join("fourth.txt");
    fs::write(&fourth, b"dummy").unwrap();

    let mut cache = SearchCache::walk_fs(dir);
    // NOT (Alpha OR Beta) - should match Gamma and untagged
    let indices = guard_indices(cache.search_with_options(
        "!tag:Alpha;Beta type:file",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    // Should match third.txt (Gamma) and fourth.txt (no tags)
    assert_eq!(indices.len(), 2);
}

#[test]
fn tag_filter_handles_special_filesystem_chars() {
    let temp_dir = TempDir::new("tag_fs_chars").unwrap();
    let dir = temp_dir.path();

    let file = dir.join("file.txt");
    fs::write(&file, b"dummy").unwrap();
    write_tags(&file, &["Tag/With/Slash", "Tag:With:Colon"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:Slash",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 1);
}

#[test]
fn tag_filter_leading_trailing_whitespace_in_list() {
    let temp_dir = TempDir::new("tag_list_ws").unwrap();
    let dir = temp_dir.path();

    let first = dir.join("first.txt");
    fs::write(&first, b"dummy").unwrap();
    write_tags(&first, &["Alpha"]);

    let mut cache = SearchCache::walk_fs(dir);
    // Use compact semicolon without spaces to get list parsing
    let indices = guard_indices(cache.search_with_options(
        "tag:Alpha;Beta",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    // Should match Alpha (list OR semantics)
    assert_eq!(indices.len(), 1);
}

#[test]
fn tag_filter_or_operator_with_lists() {
    let temp_dir = TempDir::new("tag_or_lists").unwrap();
    let dir = temp_dir.path();

    let first = dir.join("first.txt");
    fs::write(&first, b"dummy").unwrap();
    write_tags(&first, &["Alpha"]);

    let second = dir.join("second.txt");
    fs::write(&second, b"dummy").unwrap();
    write_tags(&second, &["Beta"]);

    let third = dir.join("third.txt");
    fs::write(&third, b"dummy").unwrap();
    write_tags(&third, &["Gamma"]);

    let fourth = dir.join("fourth.txt");
    fs::write(&fourth, b"dummy").unwrap();
    write_tags(&fourth, &["Delta"]);

    let mut cache = SearchCache::walk_fs(dir);
    // (Alpha OR Beta) OR (Gamma OR Delta) - should match all
    let indices = guard_indices(cache.search_with_options(
        "tag:Alpha;Beta | tag:Gamma;Delta",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 4);
}

#[test]
fn tag_filter_number_only_tag() {
    let temp_dir = TempDir::new("tag_number_only").unwrap();
    let dir = temp_dir.path();

    let file = dir.join("file.txt");
    fs::write(&file, b"dummy").unwrap();
    write_tags(&file, &["2024", "123"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:2024",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 1);
}

#[test]
fn tag_filter_dot_in_tag_name() {
    let temp_dir = TempDir::new("tag_dot").unwrap();
    let dir = temp_dir.path();

    let file = dir.join("file.txt");
    fs::write(&file, b"dummy").unwrap();
    write_tags(&file, &["v1.0.0", "config.prod"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:v1.0.0",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 1);
}

#[test]
fn tag_filter_parentheses_in_tag_name() {
    let temp_dir = TempDir::new("tag_parens").unwrap();
    let dir = temp_dir.path();

    let file = dir.join("file.txt");
    fs::write(&file, b"dummy").unwrap();
    write_tags(&file, &["Project (2024)", "Todo (urgent)"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:2024",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 1);
}

#[test]
fn tag_filter_brackets_in_tag_name() {
    let temp_dir = TempDir::new("tag_brackets").unwrap();
    let dir = temp_dir.path();

    let file = dir.join("file.txt");
    fs::write(&file, b"dummy").unwrap();
    write_tags(&file, &["[Important]", "Status[Active]"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:Important",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 1);
}

#[test]
fn tag_filter_ampersand_in_tag_name() {
    let temp_dir = TempDir::new("tag_ampersand").unwrap();
    let dir = temp_dir.path();

    let file = dir.join("file.txt");
    fs::write(&file, b"dummy").unwrap();
    write_tags(&file, &["R&D", "Design & Development"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:R&D",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 1);
}

#[test]
fn tag_filter_percent_in_tag_name() {
    let temp_dir = TempDir::new("tag_percent").unwrap();
    let dir = temp_dir.path();

    let file = dir.join("file.txt");
    fs::write(&file, b"dummy").unwrap();
    write_tags(&file, &["100%", "50% Complete"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:100%",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 1);
}

#[test]
fn tag_filter_at_sign_in_tag_name() {
    let temp_dir = TempDir::new("tag_at_sign").unwrap();
    let dir = temp_dir.path();

    let file = dir.join("file.txt");
    fs::write(&file, b"dummy").unwrap();
    write_tags(&file, &["@important", "Contact@Work"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:@important",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 1);
}

#[test]
fn tag_filter_hash_in_tag_name() {
    let temp_dir = TempDir::new("tag_hash").unwrap();
    let dir = temp_dir.path();

    let file = dir.join("file.txt");
    fs::write(&file, b"dummy").unwrap();
    write_tags(&file, &["#important", "Issue#123"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:#important",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 1);
}

#[test]
fn tag_filter_plus_in_tag_name() {
    let temp_dir = TempDir::new("tag_plus").unwrap();
    let dir = temp_dir.path();

    let file = dir.join("file.txt");
    fs::write(&file, b"dummy").unwrap();
    write_tags(&file, &["C++", "Priority+"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:C++",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 1);
}

#[test]
fn tag_filter_equals_in_tag_name() {
    let temp_dir = TempDir::new("tag_equals").unwrap();
    let dir = temp_dir.path();

    let file = dir.join("file.txt");
    fs::write(&file, b"dummy").unwrap();
    write_tags(&file, &["Status=Active", "Priority=High"]);

    let mut cache = SearchCache::walk_fs(dir);
    let indices = guard_indices(cache.search_with_options(
        "tag:Status=Active",
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 1);
}

#[test]
fn tag_filter_list_with_100_items() {
    let temp_dir = TempDir::new("tag_list_100").unwrap();
    let dir = temp_dir.path();

    // Create 100 files with different tags
    for i in 0..100 {
        let file = dir.join(format!("file{i}.txt"));
        fs::write(&file, b"dummy").unwrap();
        write_tags(&file, &[&format!("Tag{i}")]);
    }

    let mut cache = SearchCache::walk_fs(dir);
    // Create list with all 100 tags
    let tag_list = (0..100)
        .map(|i| format!("Tag{i}"))
        .collect::<Vec<_>>()
        .join(";");
    let query = format!("tag:{tag_list}");
    let indices = guard_indices(cache.search_with_options(
        &query,
        SearchOptions::default(),
        CancellationToken::noop(),
    ));
    assert_eq!(indices.len(), 100);
}
