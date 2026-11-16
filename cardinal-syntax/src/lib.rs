//! # Cardinal's Everything-like Syntax Parser
//!
//! `cardinal-syntax` turns raw Everything-style queries into a structured AST so
//! the rest of Cardinal can reason about filters, boolean logic, and phrases
//! without duplicating the parsing rules from the original Windows tool. Any
//! example shown in that manual should be accepted by [`parse_query`].
//!
//! ## Example
//! ```
//! use cardinal_syntax::{optimize_query, parse_query, Expr, FilterKind, Term};
//!
//! let parsed = parse_query("folder: dm:pastmonth ext:docx report").unwrap();
//! if let Expr::And(parts) = &parsed.expr {
//!     // parser preserves the original order
//!     assert!(matches!(&parts[0], Expr::Term(Term::Filter(filter)) if matches!(filter.kind, FilterKind::Folder)));
//!     assert!(matches!(&parts[1], Expr::Term(Term::Filter(filter)) if matches!(filter.kind, FilterKind::DateModified)));
//!     assert!(matches!(&parts[2], Expr::Term(Term::Filter(filter)) if matches!(filter.kind, FilterKind::Ext)));
//!     assert!(matches!(&parts[3], Expr::Term(Term::Word(word)) if word == "report"));
//! }
//!
//! let query = optimize_query(parsed);
//! if let Expr::And(parts) = query.expr {
//!     // optimizer pushes all filters to the end
//!     assert_eq!(parts.len(), 4);
//!     assert!(matches!(&parts[0], Expr::Term(Term::Word(word)) if word == "report"));
//!     assert!(matches!(&parts[1], Expr::Term(Term::Filter(filter)) if matches!(filter.kind, FilterKind::Folder)));
//!     assert!(matches!(&parts[2], Expr::Term(Term::Filter(filter)) if matches!(filter.kind, FilterKind::DateModified)));
//!     assert!(matches!(&parts[3], Expr::Term(Term::Filter(filter)) if matches!(filter.kind, FilterKind::Ext)));
//! }
//! ```

use std::fmt;

/// Parses an Everything-like query string into a structured expression tree.
pub fn parse_query(input: &str) -> Result<Query, ParseError> {
    Parser::new(input).parse()
}

/// User input normalized into a single expression tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub expr: Expr,
}

impl Query {
    pub fn is_empty(&self) -> bool {
        matches!(self.expr, Expr::Empty)
    }
}

/// Applies deterministic rewrites that make downstream evaluation cheaper.
///
/// The optimizer is intentionally separate from [`parse_query`] so callers can
/// choose whether they want the raw Everything AST or a normalized shape that:
/// - Removes `Expr::Empty` operands from conjunctions (returning `Expr::Empty`
///   or the lone operand when appropriate).
/// - Reorders filters by cost: `infolder:` and `parent:` first (same priority),
///   other filters next, and `tag:` always last. Non-filters stay between the
///   scope filters and the remaining filter tail.
/// - Collapses any OR chain containing `Expr::Empty` into a single
///   `Expr::Empty`, matching Cardinal's "empty means whole universe" semantics.
///
/// The function never mutates the input query in place; a new tree is returned
/// so upstream caches can keep the parsed form if needed.
pub fn optimize_query(mut query: Query) -> Query {
    query.expr = optimize_expr(query.expr);
    query
}

fn optimize_expr(expr: Expr) -> Expr {
    match expr {
        Expr::And(parts) => optimize_and(parts),
        Expr::Or(parts) => optimize_or(parts),
        Expr::Not(inner) => Expr::Not(Box::new(optimize_expr(*inner))),
        Expr::Term(_) | Expr::Empty => expr,
    }
}

/// Normalizes AND expressions by eliding `Expr::Empty`, flattening single-item
/// conjunctions, and reordering filters to the end of the chain.
fn optimize_and(parts: Vec<Expr>) -> Expr {
    let mut flattened = Vec::new();
    for expr in parts.into_iter().map(optimize_expr) {
        match expr {
            Expr::Empty => {}
            Expr::And(nested) => flattened.extend(nested),
            other => flattened.push(other),
        }
    }

    match flattened.len() {
        0 => Expr::Empty,
        1 => flattened.pop().unwrap(),
        _ => {
            reorder_by_priority(&mut flattened);
            Expr::And(flattened)
        }
    }
}

fn optimize_or(parts: Vec<Expr>) -> Expr {
    let mut flattened = Vec::new();
    let mut has_empty = false;

    for expr in parts.into_iter().map(optimize_expr) {
        match expr {
            Expr::Empty => {
                has_empty = true;
                break;
            }
            Expr::Or(nested) => flattened.extend(nested),
            other => flattened.push(other),
        }
    }

    if has_empty {
        Expr::Empty
    } else if flattened.len() == 1 {
        flattened.pop().unwrap()
    } else {
        Expr::Or(flattened)
    }
}

/// Reorders expression parts by priority to optimize query evaluation.
///
/// Priority levels (lower executes first):
/// - 0: Scope filters (`infolder:`, `parent:`) - narrow search space first
/// - 1: Non-filter terms (words, phrases, boolean ops) - cheap string matching
/// - 2: Generic filters (`ext:`, `type:`, `size:`, etc.) - moderate cost
/// - 3: Tag filters (`tag:`) - expensive metadata access, runs last
fn reorder_by_priority(parts: &mut Vec<Expr>) {
    if parts.len() <= 1 {
        return;
    }

    let priority = |expr: &Expr| -> u8 {
        match expr {
            Expr::Term(Term::Filter(filter)) => match filter.kind {
                FilterKind::InFolder | FilterKind::Parent => 0,
                FilterKind::Tag => 3,
                _ => 2,
            },
            _ => 1,
        }
    };

    let mut keyed: Vec<_> = parts
        .drain(..)
        .map(|expr| (priority(&expr), expr))
        .collect();

    keyed.sort_by_key(|(prio, _)| *prio);

    parts.extend(keyed.into_iter().map(|(_, expr)| expr));
}

/// Logical structure for Everything queries.
///
/// The `And`/`Or` variants store flat vectors instead of binary trees so
/// consumers can iterate terms without rebalancing. `Empty` exists so parsing
/// helpers can return a sentinel when a group contains whitespace or is
/// mid-construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// Returned when a query (or sub query) only contains whitespace.
    ///
    /// ```
    /// use cardinal_syntax::{parse_query, Expr};
    /// assert!(matches!(parse_query("   ").unwrap().expr, Expr::Empty));
    /// ```
    Empty,
    /// Wraps a [`Term`] so it can participate in boolean expressions.
    ///
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term};
    /// let expr = parse_query("report").unwrap().expr;
    /// assert!(matches!(expr, Expr::Term(Term::Word(word)) if word == "report"));
    /// ```
    Term(Term),
    /// Logical negation. Multiple NOT prefixes collapse into a single node.
    ///
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term};
    /// let expr = parse_query("!temp").unwrap().expr;
    /// if let Expr::Not(inner) = expr {
    ///     assert!(matches!(&*inner, Expr::Term(Term::Word(word)) if word == "temp"));
    /// }
    /// ```
    Not(Box<Expr>),
    /// Conjunction (implicit via whitespace in Everything syntax).
    ///
    /// ```
    /// use cardinal_syntax::{parse_query, Expr};
    /// let Expr::And(parts) = parse_query("foo bar").unwrap().expr else { panic!() };
    /// assert_eq!(parts.len(), 2);
    /// ```
    And(Vec<Expr>),
    /// Disjunction triggered by the `|` operator.
    ///
    /// ```
    /// use cardinal_syntax::{parse_query, Expr};
    /// let Expr::Or(parts) = parse_query("foo|bar").unwrap().expr else { panic!() };
    /// assert_eq!(parts.len(), 2);
    /// ```
    Or(Vec<Expr>),
}

/// A leaf expression that Everything understands without further boolean
/// structure. Filters are kept separate from raw words so higher layers can
/// translate them into structured lookups or validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    /// A bare word or wildcard token (e.g., `report`, `*.mp3`).
    /// Quoted phrases are preserved as `"`-wrapped text (e.g., `"summer holiday"`).
    ///
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term};
    /// let Expr::Term(Term::Word(word)) = parse_query("*.mp3").unwrap().expr else { panic!() };
    /// assert_eq!(word, "*.mp3");
    /// ```
    Word(String),
    /// `name:argument` style filters (`size:>1GB`, `folder:` ...).
    ///
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("size:>1GB").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Size));
    /// ```
    Filter(Filter),
    /// Regex-prefixed query (`regex:...`). Everything treats the rest of the
    /// string as a regular expression.
    ///
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term};
    /// let Expr::Term(Term::Regex(pattern)) = parse_query("regex:^Report").unwrap().expr else { panic!() };
    /// assert_eq!(pattern, "^Report");
    /// ```
    Regex(String),
}

/// `name:argument` style filters Everything exposes (e.g. `size:>1gb`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    pub kind: FilterKind,
    pub argument: Option<FilterArgument>,
}

mod filter_kind;
mod filter_types;
mod parser_helpers;

pub use filter_kind::FilterKind;
pub use filter_types::{
    ArgumentKind, ComparisonOp, ComparisonValue, FilterArgument, RangeSeparator, RangeValue,
};
use parser_helpers::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub position: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at byte {})", self.message, self.position)
    }
}

impl std::error::Error for ParseError {}

/// Hand-rolled recursive-descent parser because the language is tiny and we
/// need to model whitespace, booleans, and filters exactly the way Everything
/// does. Keeping the parser explicit makes it easy to line up code with the
/// textual manual.
struct Parser<'a> {
    input: &'a str,
    pos: usize,
    group_stack: Vec<char>,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            group_stack: Vec::new(),
        }
    }

    fn parse(mut self) -> Result<Query, ParseError> {
        let expr = self.parse_and()?;
        self.skip_ws();
        if !self.eof() {
            return Err(self.error("unexpected trailing characters"));
        }
        Ok(Query { expr })
    }

    // AND has the lowest precedence and is implicit between whitespace-delimited
    // terms. We accumulate a Vec instead of nesting binary nodes so callers get
    // a normalized structure regardless of how many terms are chained.
    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut parts = Vec::new();
        let mut pending_keyword_and = false;
        loop {
            self.skip_ws();
            if self.consume_keyword("AND") {
                if parts.is_empty() {
                    parts.push(Expr::Empty);
                }
                pending_keyword_and = true;
                continue;
            }
            if self.eof() || self.is_at_group_close() {
                if pending_keyword_and {
                    parts.push(Expr::Empty);
                }
                break;
            }
            let expr = self.parse_or()?;
            if matches!(expr, Expr::Empty) {
                break;
            }
            parts.push(expr);
            if pending_keyword_and {
                pending_keyword_and = false;
            }
        }

        match parts.len() {
            0 => Ok(Expr::Empty),
            1 => Ok(parts.remove(0)),
            _ => Ok(Expr::And(parts)),
        }
    }

    // OR has higher precedence than AND but still associates left-to-right via
    // the vector accumulator, mirroring how Everything evaluates `|`.
    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut parts = Vec::new();
        loop {
            self.skip_ws();
            let operand_is_empty =
                self.peek_char() == Some('|') || self.eof() || self.is_at_group_close();
            if operand_is_empty {
                parts.push(Expr::Empty);
            } else {
                parts.push(self.parse_not()?);
            }

            self.skip_ws();
            let matched = if self.peek_char() == Some('|') {
                self.advance_char();
                true
            } else {
                self.consume_keyword("OR")
            };

            if !matched {
                break;
            }
        }

        if parts.len() == 1 {
            Ok(parts.remove(0))
        } else {
            Ok(Expr::Or(parts))
        }
    }

    // NOT binds tighter than OR/AND and Everything allows chains like
    // `!!!foo`, so we count prefixes and only wrap once if the parity is odd.
    fn parse_not(&mut self) -> Result<Expr, ParseError> {
        let mut negations = 0;
        loop {
            self.skip_ws();
            if self.peek_char() == Some('!') {
                self.advance_char();
                negations += 1;
                continue;
            }
            if self.consume_keyword("NOT") {
                negations += 1;
                continue;
            }
            break;
        }

        let mut expr = self.parse_primary()?;
        if negations % 2 == 1 {
            expr = Expr::Not(Box::new(expr));
        }
        Ok(expr)
    }

    // Primary expressions cover grouped subqueries, regex, and bare tokens/filters.
    // Quoted phrases are treated as part of word tokens. Everything does not require escape sequences inside
    // quoted strings, so we treat backslashes literally.
    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        self.skip_ws();
        if self.eof() {
            return Ok(Expr::Empty);
        }

        match self.peek_char().unwrap() {
            '<' => self.parse_group('>'),
            '(' => self.parse_group(')'),
            '>' | ')' => Err(self.error("unexpected closing delimiter")),
            _ => {
                let term = self.parse_word_like()?;
                match &term {
                    Term::Word(text) if text == "\"\"" => Ok(Expr::Empty),
                    _ => Ok(Expr::Term(term)),
                }
            }
        }
    }

    fn parse_group(&mut self, closing: char) -> Result<Expr, ParseError> {
        self.advance_char(); // consume opening token
        self.group_stack.push(closing);
        let expr = self.parse_and()?;
        self.group_stack.pop();
        self.skip_ws();
        if self.peek_char() == Some(closing) {
            self.advance_char();
            Ok(expr)
        } else {
            Err(self.error(format!("expected '{closing}'")))
        }
    }

    // Parses bare tokens with optional `name:` filters. Everything allows
    // filters to appear anywhere, so this routine needs to stop as soon as we
    // see `:` to avoid consuming the argument.
    fn parse_word_like(&mut self) -> Result<Term, ParseError> {
        let start = self.pos;
        let mut seen = false;
        while let Some(ch) = self.peek_char() {
            if ch == '\\' && self.peek_next_char() == Some('"') {
                seen = true;
                self.advance_char();
                self.advance_char();
                continue;
            }
            if ch == '"' {
                self.advance_char();
                let mut closed = false;
                let mut escaped = false;
                while let Some(next) = self.peek_char() {
                    self.advance_char();
                    if escaped {
                        escaped = false;
                        continue;
                    }
                    if next == '\\' {
                        escaped = true;
                        continue;
                    }
                    if next == '"' {
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    return Err(self.error("missing closing quote"));
                }
                seen = true;
                continue;
            }

            if ch == ':' && seen {
                let name = &self.input[start..self.pos];
                if is_valid_filter_name(name) {
                    self.advance_char();
                    return self.parse_filter_term(name.to_string());
                }
            }

            if is_term_breaker(ch) {
                break;
            }
            seen = true;
            self.advance_char();
        }

        if start == self.pos {
            return Err(self.error("expected term"));
        }

        let text = self.input[start..self.pos].to_string();
        Ok(Term::Word(text))
    }

    // After seeing `name:`, decide whether this is the regex prefix (which
    // switches the entire query into regex mode) or a normal filter.
    fn parse_filter_term(&mut self, name: String) -> Result<Term, ParseError> {
        if name.eq_ignore_ascii_case("regex") {
            let pattern = self.parse_regex_pattern()?;
            return Ok(Term::Regex(pattern));
        }

        let kind = FilterKind::from_name(&name);
        let argument = self.parse_filter_argument(&kind)?;
        Ok(Term::Filter(Filter { kind, argument }))
    }

    fn parse_regex_pattern(&mut self) -> Result<String, ParseError> {
        self.skip_ws();
        if self.eof() || self.is_at_group_close() {
            return Err(self.error("regex: requires a pattern"));
        }

        if self.peek_char() == Some('"') {
            return self.parse_phrase_string();
        }

        let mut pattern = String::new();
        let mut escaped = false;
        let mut paren_depth = 0;
        let mut bracket_depth = 0;
        while let Some(ch) = self.peek_char() {
            if !escaped {
                if ch == '\\' {
                    escaped = true;
                    pattern.push(ch);
                    self.advance_char();
                    continue;
                }

                match ch {
                    '[' => bracket_depth += 1,
                    ']' if bracket_depth > 0 => bracket_depth -= 1,
                    '(' if bracket_depth == 0 => paren_depth += 1,
                    ')' if bracket_depth == 0 => {
                        if paren_depth > 0 {
                            paren_depth -= 1;
                        } else if self.current_closer_is(ch) {
                            break;
                        }
                    }
                    ch if ch.is_whitespace() && bracket_depth == 0 => break,
                    ch if self.current_closer_is(ch) && bracket_depth == 0 && paren_depth == 0 => {
                        break;
                    }
                    _ => {}
                }
            } else {
                escaped = false;
            }

            pattern.push(ch);
            self.advance_char();
        }

        if pattern.is_empty() {
            return Err(self.error("regex: requires a pattern"));
        }

        Ok(pattern)
    }

    // Extracts the argument immediately following `name:`. This function is
    // deliberately permissive because Everything accepts many free-form tokens
    // (dates, macros, etc.), so we only stop once whitespace or another control
    // character appears.
    fn parse_filter_argument(
        &mut self,
        kind: &FilterKind,
    ) -> Result<Option<FilterArgument>, ParseError> {
        if self.eof() {
            return Ok(None);
        }

        if self.peek_char().is_some_and(|ch| ch.is_whitespace()) {
            return Ok(None);
        }

        if self.starts_with_filter_token() {
            return Ok(None);
        }

        if self.is_at_group_close() || matches!(self.peek_char(), Some('|')) {
            return Ok(None);
        }

        let start = self.pos;
        let mut buffer = String::new();
        let mut is_quoted = false;

        // Check if this starts with a quote - might be a quoted phrase or quoted list
        if self.peek_char() == Some('"') {
            is_quoted = true;
        }

        // Read the entire argument, handling quotes properly
        while let Some(ch) = self.peek_char() {
            if ch == '"' {
                // Read the quoted section
                let quoted = self.parse_phrase_string()?;
                buffer.push('"');
                buffer.push_str(&quoted);
                buffer.push('"');
                continue;
            }

            if ch.is_whitespace() || ch == '|' {
                break;
            }

            if self.current_closer_is(ch) {
                break;
            }

            if (ch == '<' || ch == '>') && buffer.is_empty() {
                buffer.push(ch);
                self.advance_char();
                continue;
            }

            if (ch == '<' || ch == '>') && !buffer.is_empty() {
                break;
            }

            buffer.push(ch);
            self.advance_char();
        }

        if buffer.is_empty() {
            self.pos = start;
            return Ok(None);
        }

        // Empty quotes (just "\"\"") should result in None argument
        if buffer == "\"\"" {
            return Ok(None);
        }

        let argument_kind = classify_argument(kind, &buffer, is_quoted);
        Ok(Some(FilterArgument {
            raw: buffer,
            kind: argument_kind,
        }))
    }

    // Everything supports literal double-quoted phrases. We allow backslash-
    // escaped quotes so `\"` can appear inside a phrase without terminating it.
    // The backslashes are preserved in the output so callers can decide how to
    // interpret them later.
    fn parse_phrase_string(&mut self) -> Result<String, ParseError> {
        let quote_pos = self.pos;
        self.advance_char(); // opening quote
        let mut result = String::new();
        let mut escaped = false;
        while let Some(ch) = self.peek_char() {
            self.advance_char();
            if escaped {
                escaped = false;
                result.push(ch);
                continue;
            }
            if ch == '\\' {
                escaped = true;
                result.push(ch);
                continue;
            }
            if ch == '"' {
                return Ok(result);
            }
            result.push(ch);
        }

        Err(ParseError {
            message: "missing closing quote".into(),
            position: quote_pos,
        })
    }

    fn skip_ws(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() {
                self.advance_char();
            } else {
                break;
            }
        }
    }

    fn consume_keyword(&mut self, keyword: &str) -> bool {
        let rest = self.remaining();
        if rest.len() < keyword.len() || !rest.is_char_boundary(keyword.len()) {
            return false;
        }
        let candidate = &rest[..keyword.len()];
        if !candidate.eq_ignore_ascii_case(keyword) {
            return false;
        }
        if let Some(next) = rest[keyword.len()..].chars().next()
            && !is_keyword_boundary_char(next)
        {
            return false;
        }
        self.pos += keyword.len();
        true
    }

    fn current_closer_is(&self, ch: char) -> bool {
        matches!(self.group_stack.last(), Some(&closer) if closer == ch)
    }

    fn is_at_group_close(&self) -> bool {
        matches!((self.group_stack.last(), self.peek_char()), (Some(&closer), Some(ch)) if closer == ch)
    }

    fn remaining(&self) -> &'a str {
        &self.input[self.pos..]
    }

    fn peek_char(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    fn peek_next_char(&self) -> Option<char> {
        let mut chars = self.remaining().chars();
        chars.next();
        chars.next()
    }

    fn advance_char(&mut self) {
        if let Some(ch) = self.peek_char() {
            self.pos += ch.len_utf8();
        }
    }

    fn eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn error(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            message: message.into(),
            position: self.pos,
        }
    }

    // Determines whether the cursor is sitting at the next `name:` token so
    // filters such as `video: size:>1gb` don't treat the second filter as the
    // first one's argument.
    fn starts_with_filter_token(&self) -> bool {
        let rest = self.remaining();
        for (idx, ch) in rest.char_indices() {
            if ch.is_whitespace() || is_term_breaker(ch) {
                break;
            }
            if ch == ':' {
                if idx == 0 {
                    return false;
                }
                if !rest.is_char_boundary(idx) {
                    return false;
                }
                let name = &rest[..idx];
                if !is_valid_filter_name(name) {
                    return false;
                }
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
