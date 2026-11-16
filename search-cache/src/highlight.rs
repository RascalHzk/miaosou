use crate::query_preprocessor::strip_query_quotes_text;
use cardinal_syntax::{ArgumentKind, Expr, FilterArgument, Term};
use query_segmentation::{Segment, query_segmentation};
use std::collections::BTreeSet;

pub fn derive_highlight_terms(expr: &Expr) -> Vec<String> {
    let mut collector = HighlightCollector::default();
    collector.collect_expr(expr);
    collector.into_terms()
}

#[derive(Default)]
struct HighlightCollector {
    terms: BTreeSet<String>,
}

impl HighlightCollector {
    fn collect_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Empty => {}
            Expr::Term(term) => self.collect_term(term),
            Expr::Not(inner) => self.collect_expr(inner),
            Expr::And(parts) | Expr::Or(parts) => {
                for part in parts {
                    self.collect_expr(part);
                }
            }
        }
    }

    fn collect_term(&mut self, term: &Term) {
        match term {
            Term::Word(word) => self.collect_text(word),
            Term::Filter(filter) => {
                if let Some(argument) = &filter.argument {
                    self.collect_argument(argument);
                }
            }
            Term::Regex(_) => {}
        }
    }

    fn collect_argument(&mut self, argument: &FilterArgument) {
        match &argument.kind {
            ArgumentKind::Bare => self.collect_text(argument.raw.as_str()),
            ArgumentKind::Phrase => self.collect_literal_text(argument.raw.as_str()),
            ArgumentKind::List(values) => {
                for value in values {
                    if value.contains('"') {
                        self.collect_literal_text(value);
                    } else {
                        self.collect_text(value);
                    }
                }
            }
            ArgumentKind::Range(_) | ArgumentKind::Comparison(_) => {}
        }
    }

    fn collect_text(&mut self, value: &str) {
        if value.trim().is_empty() {
            return;
        }

        let segments = query_segmentation(value);
        if let Some(segment) = segments.last() {
            let segment = match segment {
                Segment::Concrete(concrete) => concrete,
                Segment::GlobStar => {
                    // "**" does not contribute to highlight terms
                    return;
                }
                Segment::Star => {
                    // "*" does not contribute to highlight terms
                    return;
                }
            };
            let candidates = literal_chunks(segment.as_value());
            if !candidates.is_empty() {
                for candidate in candidates {
                    self.push(candidate);
                }
                // Return last segment only(which is the filename part in a path)
                return;
            }
        }

        for candidate in literal_chunks(value) {
            self.push(candidate);
        }
    }

    fn collect_literal_text(&mut self, value: &str) {
        let unquoted = strip_query_quotes_text(value);
        let unquoted = unquoted.trim();
        if unquoted.is_empty() {
            return;
        }
        self.push(unquoted.to_string());
    }

    fn push(&mut self, candidate: String) {
        self.terms.insert(candidate.to_lowercase());
    }

    fn into_terms(self) -> Vec<String> {
        self.terms.into_iter().collect()
    }
}

fn literal_chunks(value: &str) -> Vec<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let chunks: Vec<String> = trimmed
        .split(['*', '?'])
        .map(str::trim)
        .filter(|chunk| !chunk.is_empty())
        .map(|chunk| chunk.to_string())
        .collect();

    if chunks.is_empty() && !trimmed.contains(['*', '?']) {
        vec![trimmed.to_string()]
    } else {
        chunks
    }
}

#[cfg(test)]
#[path = "highlight/tests/mod.rs"]
mod test_helpers;
