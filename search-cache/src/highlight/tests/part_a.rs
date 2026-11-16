use super::*;

#[test]
fn test_empty_query() {
    let terms = parse_and_highlight("").unwrap();
    assert_eq!(terms.len(), 0);
}

#[test]
fn test_whitespace_only() {
    let terms = parse_and_highlight("   ").unwrap();
    assert_eq!(terms.len(), 0);
}

#[test]
fn test_single_word() {
    let terms = parse_and_highlight("report").unwrap();
    assert_eq!(terms, vec!["report"]);
}

#[test]
fn test_single_word_uppercase() {
    let terms = parse_and_highlight("REPORT").unwrap();
    assert_eq!(terms, vec!["report"]);
}

#[test]
fn test_single_word_mixedcase() {
    let terms = parse_and_highlight("RePoRt").unwrap();
    assert_eq!(terms, vec!["report"]);
}

#[test]
fn test_two_words() {
    let terms = parse_and_highlight("hello world").unwrap();
    assert_eq!(terms, vec!["hello", "world"]);
}

#[test]
fn test_multiple_words() {
    let terms = parse_and_highlight("foo bar baz qux").unwrap();
    assert_eq!(terms, vec!["bar", "baz", "foo", "qux"]);
}

#[test]
fn test_duplicate_words() {
    let terms = parse_and_highlight("test test test").unwrap();
    assert_eq!(terms, vec!["test"]);
}

#[test]
fn test_duplicate_words_different_case() {
    let terms = parse_and_highlight("Test TEST test TeSt").unwrap();
    assert_eq!(terms, vec!["test"]);
}

#[test]
fn test_phrase_single_word() {
    let terms = parse_and_highlight("\"hello\"").unwrap();
    assert_eq!(terms, vec!["hello"]);
}

#[test]
fn test_phrase_with_escaped_quotes() {
    let terms = parse_and_highlight(r#"\"hello\""#).unwrap();
    assert_eq!(terms, vec![r#""hello""#]);
}

#[test]
fn test_phrase_with_escaped_backslash() {
    let terms = parse_and_highlight(r#""C:\\path""#).unwrap();
    assert_eq!(terms, vec![r"c:\path"]);
}

#[test]
fn test_word_with_escaped_quotes() {
    let terms = parse_and_highlight(r#"foo\"bar\"baz"#).unwrap();
    assert_eq!(terms, vec![r#"foo"bar"baz"#]);
}

#[test]
fn test_word_with_escaped_quotes_and_backslashes() {
    let terms = parse_and_highlight(r#"\"C:\\path\""#).unwrap();
    assert_eq!(terms, vec![r#""c:\path""#]);
}

#[test]
fn test_phrase_multiple_words() {
    let terms = parse_and_highlight("\"hello world\"").unwrap();
    assert_eq!(terms, vec!["hello world"]);
}

#[test]
fn test_phrase_with_whitespace() {
    let terms = parse_and_highlight("\"  hello   world  \"").unwrap();
    assert_eq!(terms, vec!["hello   world"]);
}

#[test]
fn test_phrase_uppercase() {
    let terms = parse_and_highlight("\"HELLO WORLD\"").unwrap();
    assert_eq!(terms, vec!["hello world"]);
}

#[test]
fn test_mixed_words_and_phrases() {
    let terms = parse_and_highlight("foo \"bar baz\" qux").unwrap();
    assert_eq!(terms, vec!["bar baz", "foo", "qux"]);
}

#[test]
fn test_word_with_numbers() {
    let terms = parse_and_highlight("test123").unwrap();
    assert_eq!(terms, vec!["test123"]);
}

#[test]
fn test_word_with_underscore() {
    let terms = parse_and_highlight("hello_world").unwrap();
    assert_eq!(terms, vec!["hello_world"]);
}

#[test]
fn test_word_with_hyphen() {
    let terms = parse_and_highlight("hello-world").unwrap();
    assert_eq!(terms, vec!["hello-world"]);
}

#[test]
fn test_word_with_dot() {
    let terms = parse_and_highlight("file.txt").unwrap();
    assert_eq!(terms, vec!["file.txt"]);
}

#[test]
fn test_numbers_only() {
    let terms = parse_and_highlight("12345").unwrap();
    assert_eq!(terms, vec!["12345"]);
}

#[test]
fn test_special_characters() {
    let terms = parse_and_highlight("hello@world").unwrap();
    assert_eq!(terms, vec!["hello@world"]);
}

#[test]
fn test_unicode_text() {
    let terms = parse_and_highlight("你好世界").unwrap();
    assert_eq!(terms, vec!["你好世界"]);
}

#[test]
fn test_unicode_phrase() {
    let terms = parse_and_highlight("\"你好 世界\"").unwrap();
    assert_eq!(terms, vec!["你好 世界"]);
}

#[test]
fn test_emoji() {
    let terms = parse_and_highlight("test🔥file").unwrap();
    assert_eq!(terms, vec!["test🔥file"]);
}

#[test]
fn test_mixed_languages() {
    let terms = parse_and_highlight("hello 世界 test").unwrap();
    assert_eq!(terms, vec!["hello", "test", "世界"]);
}

#[test]
fn test_cyrillic_text() {
    let terms = parse_and_highlight("привет мир").unwrap();
    assert_eq!(terms, vec!["мир", "привет"]);
}

#[test]
fn test_arabic_text() {
    let terms = parse_and_highlight("مرحبا عالم").unwrap();
    assert_eq!(terms, vec!["عالم", "مرحبا"]);
}

#[test]
fn test_japanese_text() {
    let terms = parse_and_highlight("こんにちは 世界").unwrap();
    assert_eq!(terms, vec!["こんにちは", "世界"]);
}

#[test]
fn test_korean_text() {
    let terms = parse_and_highlight("안녕하세요 세계").unwrap();
    assert_eq!(terms, vec!["세계", "안녕하세요"]);
}

// ============================================================================
// Wildcard and Pattern Tests (200 lines)
// ============================================================================

#[test]
fn test_wildcard_star() {
    let terms = parse_and_highlight("*.txt").unwrap();
    assert_eq!(terms, vec![".txt"]);
}

#[test]
fn test_wildcard_question() {
    let terms = parse_and_highlight("file?.txt").unwrap();
    assert_eq!(terms, vec![".txt", "file"]);
}

#[test]
fn test_wildcard_both_sides() {
    let terms = parse_and_highlight("*test*").unwrap();
    assert_eq!(terms, vec!["test"]);
}

#[test]
fn test_wildcard_multiple_stars() {
    let terms = parse_and_highlight("*hello*world*").unwrap();
    assert_eq!(terms, vec!["hello", "world"]);
}

#[test]
fn test_wildcard_only_star() {
    let terms = parse_and_highlight("*").unwrap();
    assert_eq!(terms.len(), 0);
}

#[test]
fn test_wildcard_only_question() {
    let terms = parse_and_highlight("?").unwrap();
    assert_eq!(terms.len(), 0);
}

#[test]
fn test_wildcard_multiple_only() {
    let terms = parse_and_highlight("***???***").unwrap();
    assert_eq!(terms.len(), 0);
}

#[test]
fn test_wildcard_with_spaces() {
    let terms = parse_and_highlight("* test *").unwrap();
    assert_eq!(terms, vec!["test"]);
}

#[test]
fn test_wildcard_prefix() {
    let terms = parse_and_highlight("test*").unwrap();
    assert_eq!(terms, vec!["test"]);
}

#[test]
fn test_wildcard_suffix() {
    let terms = parse_and_highlight("*test").unwrap();
    assert_eq!(terms, vec!["test"]);
}

#[test]
fn test_wildcard_middle() {
    let terms = parse_and_highlight("he*lo").unwrap();
    assert_eq!(terms, vec!["he", "lo"]);
}

#[test]
fn test_wildcard_complex_pattern() {
    let terms = parse_and_highlight("*test?file*.txt").unwrap();
    assert_eq!(terms, vec![".txt", "file", "test"]);
}

#[test]
fn test_wildcard_numbers() {
    let terms = parse_and_highlight("file*123").unwrap();
    assert_eq!(terms, vec!["123", "file"]);
}

#[test]
fn test_wildcard_underscore() {
    let terms = parse_and_highlight("test_*_file").unwrap();
    assert_eq!(terms, vec!["_file", "test_"]);
}

#[test]
fn test_wildcard_hyphen() {
    let terms = parse_and_highlight("test-*-file").unwrap();
    assert_eq!(terms, vec!["-file", "test-"]);
}

#[test]
fn test_wildcard_dot() {
    let terms = parse_and_highlight("test.*.file").unwrap();
    assert_eq!(terms, vec![".file", "test."]);
}

#[test]
fn test_wildcard_unicode() {
    let terms = parse_and_highlight("*你好*").unwrap();
    assert_eq!(terms, vec!["你好"]);
}

#[test]
fn test_wildcard_emoji() {
    let terms = parse_and_highlight("*🔥*").unwrap();
    assert_eq!(terms, vec!["🔥"]);
}

#[test]
fn test_multiple_wildcards_separate_words() {
    let terms = parse_and_highlight("*.txt *.rs").unwrap();
    assert_eq!(terms, vec![".rs", ".txt"]);
}

#[test]
fn test_wildcard_in_phrase() {
    let terms = parse_and_highlight("\"test * file\"").unwrap();
    assert_eq!(terms, vec!["file", "test"]);
}

#[test]
fn test_home_expansion_in_highlights() {
    let Ok(_) = std::env::var("HOME") else {
        return;
    };
    let terms = parse_and_highlight("~").unwrap();

    assert_ne!(terms, vec!["~"]);
    assert!(terms.iter().all(|term| !term.contains('~')));
}

#[test]
fn test_path_with_wildcard() {
    let terms = parse_and_highlight("src/*/test.rs").unwrap();
    assert_eq!(terms, vec!["test.rs"]);
}

#[test]
fn test_extension_wildcard() {
    let terms = parse_and_highlight("file.*").unwrap();
    assert_eq!(terms, vec!["file."]);
}

#[test]
fn test_basename_wildcard() {
    let terms = parse_and_highlight("*.tar.gz").unwrap();
    assert_eq!(terms, vec![".tar.gz"]);
}

#[test]
fn test_wildcard_beginning_and_end() {
    let terms = parse_and_highlight("*file.txt*").unwrap();
    assert_eq!(terms, vec!["file.txt"]);
}

#[test]
fn test_question_mark_pattern() {
    let terms = parse_and_highlight("test???").unwrap();
    assert_eq!(terms, vec!["test"]);
}

#[test]
fn test_question_mark_middle() {
    let terms = parse_and_highlight("te?st").unwrap();
    assert_eq!(terms, vec!["st", "te"]);
}

#[test]
fn test_mixed_wildcards() {
    let terms = parse_and_highlight("*test?file*").unwrap();
    assert_eq!(terms, vec!["file", "test"]);
}

#[test]
fn test_wildcard_longest_segment() {
    let terms = parse_and_highlight("a*bb*ccc*dddd").unwrap();
    assert_eq!(terms, vec!["a", "bb", "ccc", "dddd"]);
}

#[test]
fn test_whitespace_around_wildcard() {
    let terms = parse_and_highlight("  *test*  ").unwrap();
    assert_eq!(terms, vec!["test"]);
}

// ============================================================================
// Boolean Expression Tests (200 lines)
// ============================================================================

#[test]
fn test_and_expression() {
    let terms = parse_and_highlight("foo bar").unwrap();
    assert_eq!(terms, vec!["bar", "foo"]);
}

#[test]
fn test_or_expression() {
    let terms = parse_and_highlight("foo|bar").unwrap();
    assert_eq!(terms, vec!["bar", "foo"]);
}

#[test]
fn test_not_expression() {
    let terms = parse_and_highlight("!foo").unwrap();
    assert_eq!(terms, vec!["foo"]);
}

#[test]
fn test_not_word() {
    let terms = parse_and_highlight("test !exclude").unwrap();
    assert_eq!(terms, vec!["exclude", "test"]);
}

#[test]
fn test_multiple_not() {
    let terms = parse_and_highlight("foo !bar !baz").unwrap();
    assert_eq!(terms, vec!["bar", "baz", "foo"]);
}

#[test]
fn test_complex_and_or() {
    let terms = parse_and_highlight("foo bar|baz qux").unwrap();
    assert_eq!(terms, vec!["bar", "baz", "foo", "qux"]);
}

#[test]
fn test_grouped_expression() {
    let terms = parse_and_highlight("(foo bar)").unwrap();
    assert_eq!(terms, vec!["bar", "foo"]);
}

#[test]
fn test_nested_groups() {
    let terms = parse_and_highlight("((foo bar))").unwrap();
    assert_eq!(terms, vec!["bar", "foo"]);
}

#[test]
fn test_group_with_or() {
    let terms = parse_and_highlight("(foo|bar) baz").unwrap();
    assert_eq!(terms, vec!["bar", "baz", "foo"]);
}

#[test]
fn test_group_with_not() {
    let terms = parse_and_highlight("!(foo bar)").unwrap();
    assert_eq!(terms, vec!["bar", "foo"]);
}

#[test]
fn test_multiple_groups() {
    let terms = parse_and_highlight("(foo bar) (baz qux)").unwrap();
    assert_eq!(terms, vec!["bar", "baz", "foo", "qux"]);
}

#[test]
fn test_or_with_three_terms() {
    let terms = parse_and_highlight("foo|bar|baz").unwrap();
    assert_eq!(terms, vec!["bar", "baz", "foo"]);
}

#[test]
fn test_and_with_or() {
    let terms = parse_and_highlight("foo bar|baz").unwrap();
    assert_eq!(terms, vec!["bar", "baz", "foo"]);
}

#[test]
fn test_complex_boolean() {
    let terms = parse_and_highlight("(foo|bar) baz !qux").unwrap();
    assert_eq!(terms, vec!["bar", "baz", "foo", "qux"]);
}

#[test]
fn test_not_group() {
    let terms = parse_and_highlight("foo !(bar|baz)").unwrap();
    assert_eq!(terms, vec!["bar", "baz", "foo"]);
}

#[test]
fn test_deep_nesting() {
    let terms = parse_and_highlight("((foo|(bar baz)))").unwrap();
    assert_eq!(terms, vec!["bar", "baz", "foo"]);
}

#[test]
fn test_boolean_with_wildcards() {
    let terms = parse_and_highlight("*.txt|*.rs").unwrap();
    assert_eq!(terms, vec![".rs", ".txt"]);
}

#[test]
fn test_boolean_with_phrases() {
    let terms = parse_and_highlight("\"hello world\"|\"foo bar\"").unwrap();
    assert_eq!(terms, vec!["foo bar", "hello world"]);
}

#[test]
fn test_and_with_phrases() {
    let terms = parse_and_highlight("\"hello world\" test").unwrap();
    assert_eq!(terms, vec!["hello world", "test"]);
}

#[test]
fn test_not_phrase() {
    let terms = parse_and_highlight("!\"hello world\"").unwrap();
    assert_eq!(terms, vec!["hello world"]);
}

#[test]
fn test_multiple_or_chains() {
    let terms = parse_and_highlight("a|b c|d").unwrap();
    assert_eq!(terms, vec!["a", "b", "c", "d"]);
}

#[test]
fn test_boolean_unicode() {
    let terms = parse_and_highlight("你好|世界").unwrap();
    assert_eq!(terms, vec!["世界", "你好"]);
}

#[test]
fn test_empty_group() {
    assert!(parse_for_error_check("foo () bar").is_err());
}

#[test]
fn test_whitespace_in_group() {
    let terms = parse_and_highlight("(   foo   bar   )").unwrap();
    assert_eq!(terms, vec!["bar", "foo"]);
}

#[test]
fn test_multiple_not_operators() {
    let terms = parse_and_highlight("!!foo").unwrap();
    assert_eq!(terms, vec!["foo"]);
}

#[test]
fn test_not_empty() {
    let terms = parse_and_highlight("!").unwrap();
    assert_eq!(terms.len(), 0);
}

#[test]
fn test_or_empty() {
    let terms = parse_and_highlight("|").unwrap();
    assert_eq!(terms.len(), 0);
}

#[test]
fn test_complex_nested_boolean() {
    let terms = parse_and_highlight("((a|b) (c|d)) | ((e|f) (g|h))").unwrap();
    assert_eq!(terms, vec!["a", "b", "c", "d", "e", "f", "g", "h"]);
}

#[test]
fn test_boolean_with_numbers() {
    let terms = parse_and_highlight("123|456 789").unwrap();
    assert_eq!(terms, vec!["123", "456", "789"]);
}

// ============================================================================
// Filter Tests (200 lines)
// ============================================================================

#[test]
fn test_filter_bare_argument() {
    let terms = parse_and_highlight("ext:txt").unwrap();
    assert_eq!(terms, vec!["txt"]);
}

#[test]
fn test_filter_phrase_argument() {
    let terms = parse_and_highlight("ext:\"tar gz\"").unwrap();
    assert_eq!(terms, vec!["tar gz"]);
}

#[test]
fn test_filter_no_argument() {
    let terms = parse_and_highlight("file:").unwrap();
    assert_eq!(terms.len(), 0);
}

#[test]
fn test_filter_with_word() {
    let terms = parse_and_highlight("ext:txt test").unwrap();
    assert_eq!(terms, vec!["test", "txt"]);
}

#[test]
fn test_multiple_filters() {
    let terms = parse_and_highlight("ext:txt size:>1mb").unwrap();
    assert_eq!(terms, vec!["txt"]);
}

#[test]
fn test_filter_wildcard_argument() {
    let terms = parse_and_highlight("ext:t*t").unwrap();
    assert_eq!(terms, vec!["t"]);
}

#[test]
fn test_filter_uppercase() {
    let terms = parse_and_highlight("ext:TXT").unwrap();
    assert_eq!(terms, vec!["txt"]);
}

#[test]
fn test_filter_numbers() {
    let terms = parse_and_highlight("ext:mp3").unwrap();
    assert_eq!(terms, vec!["mp3"]);
}

#[test]
fn test_filter_path() {
    let terms = parse_and_highlight("path:src/test").unwrap();
    assert_eq!(terms, vec!["test"]);
}

#[test]
fn test_filter_unicode() {
    let terms = parse_and_highlight("name:你好").unwrap();
    assert_eq!(terms, vec!["你好"]);
}

#[test]
fn test_size_filter_no_highlight() {
    let terms = parse_and_highlight("size:>1mb").unwrap();
    assert_eq!(terms.len(), 0);
}

#[test]
fn test_date_filter_no_highlight() {
    let terms = parse_and_highlight("dm:2024-01-01").unwrap();
    assert_eq!(terms, vec!["2024-01-01"]);
}

#[test]
fn test_filter_with_boolean() {
    let terms = parse_and_highlight("ext:txt|ext:rs").unwrap();
    assert_eq!(terms, vec!["rs", "txt"]);
}

#[test]
fn test_filter_with_not() {
    let terms = parse_and_highlight("test !ext:tmp").unwrap();
    assert_eq!(terms, vec!["test", "tmp"]);
}

#[test]
fn test_filter_in_group() {
    let terms = parse_and_highlight("(ext:txt test)").unwrap();
    assert_eq!(terms, vec!["test", "txt"]);
}

#[test]
fn test_filter_phrase_with_spaces() {
    let terms = parse_and_highlight("name:\"hello world\"").unwrap();
    assert_eq!(terms, vec!["hello world"]);
}

#[test]
fn test_filter_multiple_arguments() {
    let terms = parse_and_highlight("ext:txt ext:rs ext:md").unwrap();
    assert_eq!(terms, vec!["md", "rs", "txt"]);
}

#[test]
fn test_filter_empty_argument() {
    let terms = parse_and_highlight("ext:\"\"").unwrap();
    assert_eq!(terms.len(), 0);
}

#[test]
fn test_filter_whitespace_argument() {
    let terms = parse_and_highlight("ext:\"   \"").unwrap();
    assert_eq!(terms.len(), 0);
}

#[test]
fn test_folder_filter() {
    let terms = parse_and_highlight("folder:Documents").unwrap();
    assert_eq!(terms, vec!["documents"]);
}

#[test]
fn test_file_filter_with_name() {
    let terms = parse_and_highlight("file:test.txt").unwrap();
    assert_eq!(terms, vec!["test.txt"]);
}

#[test]
fn test_type_filter() {
    let terms = parse_and_highlight("type:picture").unwrap();
    assert_eq!(terms, vec!["picture"]);
}

#[test]
fn test_filter_with_hyphen() {
    let terms = parse_and_highlight("ext:tar-gz").unwrap();
    assert_eq!(terms, vec!["tar-gz"]);
}

#[test]
fn test_filter_with_underscore() {
    let terms = parse_and_highlight("name:test_file").unwrap();
    assert_eq!(terms, vec!["test_file"]);
}

#[test]
fn test_filter_with_dot() {
    let terms = parse_and_highlight("name:file.test.txt").unwrap();
    assert_eq!(terms, vec!["file.test.txt"]);
}

#[test]
fn test_regex_filter_no_highlight() {
    let terms = parse_and_highlight("regex:test.*").unwrap();
    assert_eq!(terms.len(), 0);
}

#[test]
fn test_regex_with_other_terms() {
    let terms = parse_and_highlight("regex:test.* hello").unwrap();
    assert_eq!(terms, vec!["hello"]);
}

#[test]
fn test_filter_duplicate_values() {
    let terms = parse_and_highlight("ext:txt ext:txt").unwrap();
    assert_eq!(terms, vec!["txt"]);
}

#[test]
fn test_filter_case_insensitive_dedup() {
    let terms = parse_and_highlight("ext:TXT ext:txt ext:Txt").unwrap();
    assert_eq!(terms, vec!["txt"]);
}

#[test]
fn test_filter_with_emoji() {
    let terms = parse_and_highlight("name:test🔥file").unwrap();
    assert_eq!(terms, vec!["test🔥file"]);
}

#[test]
fn test_filter_special_chars() {
    let terms = parse_and_highlight("name:test@file.txt").unwrap();
    assert_eq!(terms, vec!["test@file.txt"]);
}

#[test]
fn test_audio_filter() {
    let terms = parse_and_highlight("audio:").unwrap();
    assert_eq!(terms.len(), 0);
}

#[test]
fn test_video_filter() {
    let terms = parse_and_highlight("video:").unwrap();
    assert_eq!(terms.len(), 0);
}

#[test]
fn test_doc_filter() {
    let terms = parse_and_highlight("doc:").unwrap();
    assert_eq!(terms.len(), 0);
}

// ============================================================================
// Edge Cases and Complex Scenarios (200 lines)
// ============================================================================

#[test]
fn test_very_long_word() {
    let long_word = "a".repeat(1000);
    let terms = parse_and_highlight(&long_word).unwrap();
    assert_eq!(terms.len(), 1);
    assert_eq!(terms[0].len(), 1000);
}

#[test]
fn test_very_long_phrase() {
    let long_phrase = format!("\"{}\"", "test ".repeat(500));
    let terms = parse_and_highlight(&long_phrase).unwrap();
    assert_eq!(terms.len(), 1);
}

#[test]
fn test_many_terms() {
    let query = (0..100)
        .map(|i| format!("word{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    let terms = parse_and_highlight(&query).unwrap();
    assert_eq!(terms.len(), 100);
}

#[test]
fn test_many_or_terms() {
    let query = (0..50)
        .map(|i| format!("word{i}"))
        .collect::<Vec<_>>()
        .join("|");
    let terms = parse_and_highlight(&query).unwrap();
    assert_eq!(terms.len(), 50);
}

#[test]
fn test_deeply_nested_groups() {
    let mut query = String::from("test");
    for _ in 0..20 {
        query = format!("({query})");
    }
    let terms = parse_and_highlight(&query).unwrap();
    assert_eq!(terms, vec!["test"]);
}

#[test]
fn test_mixed_everything() {
    let terms =
        parse_and_highlight("*.txt \"hello world\" test !exclude ext:rs size:>1mb (foo|bar)")
            .unwrap();

    assert!(terms.contains(&".txt".to_string()));
    assert!(terms.contains(&"hello world".to_string()));
    assert!(terms.contains(&"test".to_string()));
    assert!(terms.contains(&"exclude".to_string()));
    assert!(terms.contains(&"rs".to_string()));
    assert!(terms.contains(&"foo".to_string()));
    assert!(terms.contains(&"bar".to_string()));
}

#[test]
fn test_sanitize_only_wildcards() {
    let terms = parse_and_highlight("*?*?*").unwrap();
    assert_eq!(terms.len(), 0);
}

#[test]
fn test_sanitize_trim_wildcards() {
    let terms = parse_and_highlight("***test???").unwrap();
    assert_eq!(terms, vec!["test"]);
}

#[test]
fn test_sanitize_middle_wildcards() {
    let terms = parse_and_highlight("test*ing").unwrap();
    assert_eq!(terms, vec!["ing", "test"]);
}

#[test]
fn test_empty_segments() {
    let terms = parse_and_highlight("**hello**world**").unwrap();
    assert_eq!(terms, vec!["hello", "world"]);
}

#[test]
fn test_only_whitespace_segments() {
    let terms = parse_and_highlight("   ").unwrap();
    assert_eq!(terms.len(), 0);
}

#[test]
fn test_tab_characters() {
    let terms = parse_and_highlight("hello\tworld").unwrap();
    assert_eq!(terms, vec!["hello", "world"]);
}

#[test]
fn test_newline_characters() {
    let terms = parse_and_highlight("hello\nworld").unwrap();
    assert_eq!(terms, vec!["hello", "world"]);
}

#[test]
fn test_carriage_return() {
    let terms = parse_and_highlight("hello\rworld").unwrap();
    assert_eq!(terms, vec!["hello", "world"]);
}

#[test]
fn test_mixed_whitespace() {
    let terms = parse_and_highlight("hello \t\n\r world").unwrap();
    assert_eq!(terms, vec!["hello", "world"]);
}

#[test]
fn test_zero_width_characters() {
    let terms = parse_and_highlight("test\u{200B}file").unwrap();
    assert_eq!(terms.len(), 1);
}

#[test]
fn test_combining_characters() {
    let terms = parse_and_highlight("café").unwrap();
    assert_eq!(terms.len(), 1);
}

#[test]
fn test_rtl_text() {
    let terms = parse_and_highlight("שלום עולם").unwrap();
    assert_eq!(terms.len(), 2);
}

#[test]
fn test_mixed_rtl_ltr() {
    let terms = parse_and_highlight("hello שלום world").unwrap();
    assert_eq!(terms.len(), 3);
}

#[test]
fn test_backslash_in_query() {
    let terms = parse_and_highlight("path\\to\\file").unwrap();
    assert_eq!(terms, vec!["path\\to\\file"]);
}

#[test]
fn test_forward_slash_in_query() {
    let terms = parse_and_highlight("path/to/file").unwrap();
    assert_eq!(terms, vec!["file"]);
}

#[test]
fn test_mixed_slashes() {
    let terms = parse_and_highlight("path\\to/file").unwrap();
    assert_eq!(terms, vec!["file"]);
}

#[test]
fn test_quotes_in_word() {
    let terms = parse_and_highlight("\"Trae CN\"/emm.db").unwrap();

    assert!(!terms.is_empty());
}

#[test]
fn test_quotes_in_word_adjacent() {
    assert!(parse_for_error_check("test\"file").is_err());
}

#[test]
fn test_quotes_in_word_adjacent_balanced() {
    let terms = parse_and_highlight("test\"file\"").unwrap();

    assert!(!terms.is_empty());
}

#[test]
fn test_parentheses_in_word() {
    let terms = parse_and_highlight("test(file)").unwrap();

    assert!(!terms.is_empty());
}

#[test]
fn test_brackets_in_word() {
    let terms = parse_and_highlight("test[file]").unwrap();
    assert_eq!(terms, vec!["test[file]"]);
}

#[test]
fn test_braces_in_word() {
    let terms = parse_and_highlight("test{file}").unwrap();
    assert_eq!(terms, vec!["test{file}"]);
}

#[test]
fn test_angle_brackets_in_word() {
    let terms = parse_and_highlight("test<file>").unwrap();
    assert_eq!(terms, vec!["file", "test"]);
}

#[test]
fn test_ampersand_in_word() {
    let terms = parse_and_highlight("test&file").unwrap();
    assert_eq!(terms, vec!["test&file"]);
}

#[test]
fn test_pipe_in_phrase() {
    let terms = parse_and_highlight("\"test|file\"").unwrap();
    assert_eq!(terms, vec!["test|file"]);
}

#[test]
fn test_exclamation_in_phrase() {
    let terms = parse_and_highlight("\"test!file\"").unwrap();
    assert_eq!(terms, vec!["test!file"]);
}

#[test]
fn test_colon_in_word() {
    let terms = parse_and_highlight("test:file").unwrap();
    assert_eq!(terms, vec!["file"]);
}

#[test]
fn test_semicolon_in_word() {
    let terms = parse_and_highlight("test;file").unwrap();
    assert_eq!(terms, vec!["test;file"]);
}

#[test]
fn test_comma_in_word() {
    let terms = parse_and_highlight("test,file").unwrap();
    assert_eq!(terms, vec!["test,file"]);
}

#[test]
fn test_percent_in_word() {
    let terms = parse_and_highlight("test%file").unwrap();
    assert_eq!(terms, vec!["test%file"]);
}

#[test]
fn test_dollar_in_word() {
    let terms = parse_and_highlight("test$file").unwrap();
    assert_eq!(terms, vec!["test$file"]);
}

#[test]
fn test_hash_in_word() {
    let terms = parse_and_highlight("test#file").unwrap();
    assert_eq!(terms, vec!["test#file"]);
}

#[test]
fn test_plus_in_word() {
    let terms = parse_and_highlight("test+file").unwrap();
    assert_eq!(terms, vec!["test+file"]);
}

#[test]
fn test_equals_in_word() {
    let terms = parse_and_highlight("test=file").unwrap();
    assert_eq!(terms, vec!["test=file"]);
}

#[test]
fn test_tilde_in_word() {
    let terms = parse_and_highlight("test~file").unwrap();
    assert_eq!(terms, vec!["test~file"]);
}

#[test]
fn test_backtick_in_word() {
    let terms = parse_and_highlight("test`file").unwrap();
    assert_eq!(terms, vec!["test`file"]);
}

// ============================================================================
// Query Segmentation Integration Tests (200 lines)
// ============================================================================

#[test]
fn test_segmentation_camelcase() {
    let terms = parse_and_highlight("testFile").unwrap();
    assert_eq!(terms, vec!["testfile"]);
}

#[test]
fn test_segmentation_pascalcase() {
    let terms = parse_and_highlight("TestFile").unwrap();
    assert_eq!(terms, vec!["testfile"]);
}

#[test]
fn test_segmentation_snake_case() {
    let terms = parse_and_highlight("test_file").unwrap();
    assert_eq!(terms, vec!["test_file"]);
}

#[test]
fn test_segmentation_kebab_case() {
    let terms = parse_and_highlight("test-file").unwrap();
    assert_eq!(terms, vec!["test-file"]);
}

#[test]
fn test_segmentation_dot_separated() {
    let terms = parse_and_highlight("test.file.name").unwrap();
    assert_eq!(terms, vec!["test.file.name"]);
}

#[test]
fn test_segmentation_mixed_case() {
    let terms = parse_and_highlight("TestFile_Name").unwrap();
    assert_eq!(terms, vec!["testfile_name"]);
}

#[test]
fn test_segmentation_with_numbers() {
    let terms = parse_and_highlight("test123File").unwrap();
    assert_eq!(terms, vec!["test123file"]);
}

#[test]
fn test_segmentation_all_caps() {
    let terms = parse_and_highlight("TESTFILE").unwrap();
    assert_eq!(terms, vec!["testfile"]);
}

#[test]
fn test_segmentation_alternating_case() {
    let terms = parse_and_highlight("TeSt").unwrap();
    assert_eq!(terms, vec!["test"]);
}

#[test]
fn test_segmentation_with_wildcard() {
    let terms = parse_and_highlight("test*File").unwrap();
    assert_eq!(terms, vec!["file", "test"]);
}

#[test]
fn test_segmentation_multiple_words() {
    let terms = parse_and_highlight("testFile anotherTest").unwrap();
    assert_eq!(terms, vec!["anothertest", "testfile"]);
}

#[test]
fn test_segmentation_phrase() {
    let terms = parse_and_highlight("\"testFile\"").unwrap();
    assert_eq!(terms, vec!["testfile"]);
}

#[test]
fn test_segmentation_in_filter() {
    let terms = parse_and_highlight("name:testFile").unwrap();
    assert_eq!(terms, vec!["testfile"]);
}

#[test]
fn test_segmentation_complex() {
    let terms = parse_and_highlight("myTestFile_v2").unwrap();
    assert_eq!(terms, vec!["mytestfile_v2"]);
}

#[test]
fn test_segmentation_with_path() {
    let terms = parse_and_highlight("src/testFile/index.ts").unwrap();
    assert_eq!(terms, vec!["index.ts"]);
}

#[test]
fn test_longest_segment_selection() {
    let terms = parse_and_highlight("a*bb*ccc").unwrap();
    assert_eq!(terms, vec!["a", "bb", "ccc"]);
}

#[test]
fn test_segment_with_underscores() {
    let terms = parse_and_highlight("__test__file__").unwrap();
    assert_eq!(terms, vec!["__test__file__"]);
}

#[test]
fn test_segment_with_hyphens() {
    let terms = parse_and_highlight("--test--file--").unwrap();
    assert_eq!(terms, vec!["--test--file--"]);
}

#[test]
fn test_segment_leading_numbers() {
    let terms = parse_and_highlight("123test").unwrap();
    assert_eq!(terms, vec!["123test"]);
}

#[test]
fn test_segment_trailing_numbers() {
    let terms = parse_and_highlight("test123").unwrap();
    assert_eq!(terms, vec!["test123"]);
}

#[test]
fn test_segment_only_numbers() {
    let terms = parse_and_highlight("123456").unwrap();
    assert_eq!(terms, vec!["123456"]);
}

#[test]
fn test_segment_mixed_separators() {
    let terms = parse_and_highlight("test_file-name.txt").unwrap();
    assert_eq!(terms, vec!["test_file-name.txt"]);
}

#[test]
fn test_segment_unicode_camelcase() {
    let terms = parse_and_highlight("测试File").unwrap();
    assert_eq!(terms, vec!["测试file"]);
}

#[test]
fn test_segment_emoji_separator() {
    let terms = parse_and_highlight("test🔥file").unwrap();
    assert_eq!(terms, vec!["test🔥file"]);
}

#[test]
fn test_segment_multiple_extensions() {
    let terms = parse_and_highlight("archive.tar.gz").unwrap();
    assert_eq!(terms, vec!["archive.tar.gz"]);
}

#[test]
fn test_segment_version_number() {
    let terms = parse_and_highlight("package-v1.2.3").unwrap();
    assert_eq!(terms, vec!["package-v1.2.3"]);
}

#[test]
fn test_segment_date_like() {
    let terms = parse_and_highlight("report-2024-01-15").unwrap();
    assert_eq!(terms, vec!["report-2024-01-15"]);
}

#[test]
fn test_segment_uuid_like() {
    let terms = parse_and_highlight("file-550e8400-e29b-41d4").unwrap();
    assert_eq!(terms, vec!["file-550e8400-e29b-41d4"]);
}

#[test]
fn test_segment_hash_like() {
    let terms = parse_and_highlight("commit-abc123def456").unwrap();
    assert_eq!(terms, vec!["commit-abc123def456"]);
}

#[test]
fn test_segment_url_like() {
    let terms = parse_and_highlight("https://example.com").unwrap();
    assert_eq!(terms, vec!["example.com"]);
}

#[test]
fn test_segment_email_like() {
    let terms = parse_and_highlight("user@example.com").unwrap();
    assert_eq!(terms, vec!["user@example.com"]);
}

#[test]
fn test_segment_ipv4_like() {
    let terms = parse_and_highlight("192.168.1.1").unwrap();
    assert_eq!(terms, vec!["192.168.1.1"]);
}

#[test]
fn test_segment_single_char() {
    let terms = parse_and_highlight("a").unwrap();
    assert_eq!(terms, vec!["a"]);
}

#[test]
fn test_segment_two_chars() {
    let terms = parse_and_highlight("ab").unwrap();
    assert_eq!(terms, vec!["ab"]);
}

#[test]
fn test_segment_repeated_chars() {
    let terms = parse_and_highlight("aaa").unwrap();
    assert_eq!(terms, vec!["aaa"]);
}

#[test]
fn test_segment_palindrome() {
    let terms = parse_and_highlight("racecar").unwrap();
    assert_eq!(terms, vec!["racecar"]);
}

#[test]
fn test_segment_abbreviation() {
    let terms = parse_and_highlight("USA").unwrap();
    assert_eq!(terms, vec!["usa"]);
}

#[test]
fn test_segment_acronym() {
    let terms = parse_and_highlight("HTTP").unwrap();
    assert_eq!(terms, vec!["http"]);
}

#[test]
fn test_segment_mixed_acronym() {
    let terms = parse_and_highlight("HTTPServer").unwrap();
    assert_eq!(terms, vec!["httpserver"]);
}

// ============================================================================
// Sanitization and Trimming Tests (200 lines)
// ============================================================================

#[test]
fn test_sanitize_leading_wildcards() {
    let terms = parse_and_highlight("***test").unwrap();
    assert_eq!(terms, vec!["test"]);
}

#[test]
fn test_sanitize_trailing_wildcards() {
    let terms = parse_and_highlight("test***").unwrap();
    assert_eq!(terms, vec!["test"]);
}

#[test]
fn test_sanitize_leading_questions() {
    let terms = parse_and_highlight("???test").unwrap();
    assert_eq!(terms, vec!["test"]);
}
