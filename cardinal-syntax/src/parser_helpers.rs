//! 解析器使用的自由函数（字符判定、列表/区间/比较的试探解析）。

use super::*;

pub(super) fn is_term_breaker(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '|' | '<' | '>' | '(' | ')' | '!')
}

pub(super) fn is_keyword_boundary_char(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '|' | '<' | '>' | '(' | ')' | '!' | ':' | '"' | '\\' | '/' | ';' | ',' | '.'
        )
        || matches!(ch, '[' | ']' | '{' | '}' | '#')
        || ch == '\0'
}

pub(super) fn is_valid_filter_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

/// Lightweight heuristic classification so downstream code can handle the most
/// common filter syntaxes without writing custom parsers.
pub(super) fn classify_argument(kind: &FilterKind, raw: &str, quoted: bool) -> ArgumentKind {
    // Check for list first, as it can contain quoted items
    if let Some(list) = try_parse_list(raw) {
        return ArgumentKind::List(list);
    }

    // Check for comparison
    if let Some(comparison) = try_parse_comparison(raw) {
        return ArgumentKind::Comparison(comparison);
    }

    // Check for range (important: check before defaulting to Phrase for quoted)
    if let Some(range) = try_parse_range(kind, raw) {
        return ArgumentKind::Range(range);
    }

    // If it started with a quote and isn't a list/comparison/range, it's a phrase
    if quoted {
        return ArgumentKind::Phrase;
    }

    ArgumentKind::Bare
}

/// Splits `foo;bar;baz` or `"foo";"bar";"baz"` style extension lists.
/// Semicolons inside quotes are treated as literals.
/// Escaped semicolons outside quotes (`\;`) are also treated as literals.
pub(super) fn try_parse_list(raw: &str) -> Option<Vec<String>> {
    if !raw.contains(';') {
        return None;
    }

    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_quotes = false;
    let mut escaped = false;

    for (idx, ch) in raw.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        if ch == '\\' {
            escaped = true;
            continue;
        }

        if ch == '"' {
            in_quotes = !in_quotes;
            continue;
        }

        // Only semicolons outside quotes (and not escaped) act as separators.
        if ch == ';' && !in_quotes {
            let part = raw[start..idx].trim();
            if !part.is_empty() {
                parts.push(part.to_string());
            }
            start = idx + ch.len_utf8();
        }
    }

    let tail = raw[start..].trim();
    if !tail.is_empty() {
        parts.push(tail.to_string());
    }

    (!parts.is_empty()).then_some(parts)
}

/// Detects `<, <=, >, >=, =, !=` prefixes.
pub(super) fn try_parse_comparison(raw: &str) -> Option<ComparisonValue> {
    let operators = ["<=", ">=", "!=", "<", ">", "="];
    for op in operators {
        if let Some(value) = raw.strip_prefix(op) {
            let value = value.trim().to_string();
            if value.is_empty() {
                return None;
            }
            let op_enum = match op {
                "<" => ComparisonOp::Lt,
                "<=" => ComparisonOp::Lte,
                ">" => ComparisonOp::Gt,
                ">=" => ComparisonOp::Gte,
                "=" => ComparisonOp::Eq,
                "!=" => ComparisonOp::Ne,
                _ => continue,
            };
            return Some(ComparisonValue { op: op_enum, value });
        }
    }
    None
}

/// Recognizes either dotted ranges (`a..b`) or hyphenated date ranges depending
/// on the filter name.
pub(super) fn try_parse_range(kind: &FilterKind, raw: &str) -> Option<RangeValue> {
    if let Some(range) = try_parse_dotted_range(raw) {
        return Some(range);
    }

    if allows_hyphen_range(kind)
        && let Some(range) = try_parse_hyphen_range(raw)
    {
        return Some(range);
    }

    None
}

/// Parses `start..end` expressions. Empty endpoints stay `None` to signal open
/// intervals.
pub(super) fn try_parse_dotted_range(raw: &str) -> Option<RangeValue> {
    let separator = "..";
    let index = raw.find(separator)?;
    let start_raw = raw[..index].trim();
    let end_raw = raw[index + separator.len()..].trim();
    if start_raw.is_empty() && end_raw.is_empty() {
        return None;
    }
    // Accept ranges if either side has a digit OR if both sides are quoted strings
    let has_quotes = (start_raw.starts_with('"') && start_raw.ends_with('"'))
        || (end_raw.starts_with('"') && end_raw.ends_with('"'));
    if !has_digit(start_raw) && !has_digit(end_raw) && !has_quotes {
        return None;
    }
    Some(RangeValue {
        start: if start_raw.is_empty() {
            None
        } else {
            Some(start_raw.to_string())
        },
        end: if end_raw.is_empty() {
            None
        } else {
            Some(end_raw.to_string())
        },
        separator: RangeSeparator::Dots,
    })
}

/// Hyphenated ranges are ambiguous with subtraction, so we only accept them
/// when both sides look date-like.
pub(super) fn try_parse_hyphen_range(raw: &str) -> Option<RangeValue> {
    for (idx, ch) in raw.char_indices() {
        if ch != '-' {
            continue;
        }
        let left = raw[..idx].trim();
        let right = raw[idx + 1..].trim();
        if left.is_empty() || right.is_empty() {
            continue;
        }
        if looks_like_date_fragment(left) && looks_like_date_fragment(right) {
            return Some(RangeValue {
                start: Some(left.to_string()),
                end: Some(right.to_string()),
                separator: RangeSeparator::Hyphen,
            });
        }
    }
    None
}

/// Only date-related filters accept hyphenated ranges.
pub(super) fn allows_hyphen_range(kind: &FilterKind) -> bool {
    matches!(
        kind,
        FilterKind::DateCreated
            | FilterKind::DateModified
            | FilterKind::DateAccessed
            | FilterKind::DateRun
    )
}

pub(super) fn has_digit(value: &str) -> bool {
    value.chars().any(|c| c.is_ascii_digit())
}

pub(super) fn looks_like_date_fragment(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    if !has_digit(trimmed) {
        return false;
    }
    let slash = trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains('.');
    let hyphen_count = trimmed.chars().filter(|&c| c == '-').count();
    slash || hyphen_count >= 2
}
