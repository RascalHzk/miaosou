//! 过滤器参数的类型（裸值 / 短语 / 列表 / 区间 / 比较）。

/// Captures both the raw string and the heuristically detected shape so a
/// consumer can distinguish between e.g. a list (`ext:jpg;png`) and a comparison
/// (`size:>1GB`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterArgument {
    pub raw: String,
    pub kind: ArgumentKind,
}

/// Common syntactic patterns supported by Everything filters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgumentKind {
    /// Plain argument with no additional structure.
    ///
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, ArgumentKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("folder:Projects").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.argument.unwrap().kind, ArgumentKind::Bare));
    /// ```
    Bare,
    /// Double quoted value (Everything keeps the text verbatim).
    ///
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, ArgumentKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("parent:\"/Users/demo\"").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.argument.unwrap().kind, ArgumentKind::Phrase));
    /// ```
    Phrase,
    /// Semicolon-delimited list such as `ext:jpg;png`.
    ///
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, ArgumentKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("ext:jpg;png").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.argument.unwrap().kind, ArgumentKind::List(values) if values == ["jpg", "png"]));
    /// ```
    List(Vec<String>),
    /// Numeric/date range (dotted or hyphenated).
    ///
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, ArgumentKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("size:1mb..10mb").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.argument.unwrap().kind, ArgumentKind::Range(_)));
    /// ```
    Range(RangeValue),
    /// Comparison like `>1GB`, `<=4000`, etc.
    ///
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, ArgumentKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("size:>1GB").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.argument.unwrap().kind, ArgumentKind::Comparison(_)));
    /// ```
    Comparison(ComparisonValue),
}

/// Represents `start..end` or `start-end` ranges. Empty endpoints are allowed
/// for open ranges (Everything treats `..10mb` as `<=10mb`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeValue {
    pub start: Option<String>,
    pub end: Option<String>,
    pub separator: RangeSeparator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeSeparator {
    /// Range expressed with `..` such as `size:1..10`.
    ///
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, ArgumentKind, RangeSeparator};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("size:1..10").unwrap().expr else { panic!() };
    /// let ArgumentKind::Range(range) = filter.argument.unwrap().kind else { panic!() };
    /// assert!(matches!(range.separator, RangeSeparator::Dots));
    /// ```
    Dots,
    /// Range expressed with `-` (dates such as `dc:2024/1/1-2024/12/31`).
    ///
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, ArgumentKind, RangeSeparator};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("dc:2024/01/01-2024/12/31").unwrap().expr else { panic!() };
    /// let ArgumentKind::Range(range) = filter.argument.unwrap().kind else { panic!() };
    /// assert!(matches!(range.separator, RangeSeparator::Hyphen));
    /// ```
    Hyphen,
}

/// `size:>1GB` style comparisons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonValue {
    pub op: ComparisonOp,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOp {
    /// `< value` comparison.
    ///
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, ArgumentKind, ComparisonOp};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("size:<10mb").unwrap().expr else { panic!() };
    /// let ArgumentKind::Comparison(value) = filter.argument.unwrap().kind else { panic!() };
    /// assert!(matches!(value.op, ComparisonOp::Lt));
    /// ```
    Lt,
    /// `<= value` comparison.
    ///
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, ArgumentKind, ComparisonOp};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("width:<=4000").unwrap().expr else { panic!() };
    /// let ArgumentKind::Comparison(value) = filter.argument.unwrap().kind else { panic!() };
    /// assert!(matches!(value.op, ComparisonOp::Lte));
    /// ```
    Lte,
    /// `> value` comparison.
    ///
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, ArgumentKind, ComparisonOp};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("size:>1GB").unwrap().expr else { panic!() };
    /// let ArgumentKind::Comparison(value) = filter.argument.unwrap().kind else { panic!() };
    /// assert!(matches!(value.op, ComparisonOp::Gt));
    /// ```
    Gt,
    /// `>= value` comparison.
    ///
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, ArgumentKind, ComparisonOp};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("size:>=1GB").unwrap().expr else { panic!() };
    /// let ArgumentKind::Comparison(value) = filter.argument.unwrap().kind else { panic!() };
    /// assert!(matches!(value.op, ComparisonOp::Gte));
    /// ```
    Gte,
    /// `= value` comparison.
    ///
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, ArgumentKind, ComparisonOp};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("size:=10mb").unwrap().expr else { panic!() };
    /// let ArgumentKind::Comparison(value) = filter.argument.unwrap().kind else { panic!() };
    /// assert!(matches!(value.op, ComparisonOp::Eq));
    /// ```
    Eq,
    /// `!= value` comparison.
    ///
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, ArgumentKind, ComparisonOp};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("size:!=10mb").unwrap().expr else { panic!() };
    /// let ArgumentKind::Comparison(value) = filter.argument.unwrap().kind else { panic!() };
    /// assert!(matches!(value.op, ComparisonOp::Ne));
    /// ```
    Ne,
}
