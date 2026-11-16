use super::*;
use crate::query_preprocessor::{expand_query_home_dirs, strip_query_quotes};
use cardinal_syntax::{ParseError, parse_query as raw_parse_query};

/// Helper for highlight tests: mirrors production order
/// (highlight is derived BEFORE quote stripping, preserving literal markers)
fn parse_and_highlight(input: &str) -> Result<Vec<String>, ParseError> {
    raw_parse_query(input)
        .map(expand_query_home_dirs)
        .map(strip_query_quotes)
        .map(|expanded| {
            // Derive highlights while quotes are still present (literal marker)
            derive_highlight_terms(&expanded.expr)
        })
}

/// Helper for error-checking tests that just need to parse without highlighting
fn parse_for_error_check(input: &str) -> Result<(), ParseError> {
    raw_parse_query(input).map(|_| ())
}

// ============================================================================
// Basic Word and Phrase Tests (200 lines)
// ============================================================================

mod part_a;
mod part_b;
