use super::*;

fn word(text: &str) -> Expr {
    Expr::Term(Term::Word(text.to_string()))
}

#[test]
fn parses_empty_query() {
    let query = parse_query("   ").unwrap();
    assert!(query.is_empty());
}

#[test]
fn parses_boolean_logic_with_expected_precedence() {
    let query = parse_query("foo bar|\"baz qux\" !temp").unwrap();
    assert_eq!(
        query.expr,
        Expr::And(vec![
            word("foo"),
            Expr::Or(vec![word("bar"), word("\"baz qux\"")]),
            Expr::Not(Box::new(word("temp"))),
        ])
    );
}

#[test]
fn supports_textual_boolean_keywords() {
    let query = parse_query("foo AND bar").unwrap();
    assert_eq!(query.expr, Expr::And(vec![word("foo"), word("bar")]));

    let query = parse_query("foo OR bar").unwrap();
    assert_eq!(query.expr, Expr::Or(vec![word("foo"), word("bar")]));

    let query = parse_query("NOT temp").unwrap();
    assert_eq!(query.expr, Expr::Not(Box::new(word("temp"))));

    let query = parse_query("NOT/Users").unwrap();
    assert_eq!(
        query.expr,
        Expr::Not(Box::new(Expr::Term(Term::Word("/Users".into()))))
    );
}

#[test]
fn parses_unix_style_paths() {
    let query = parse_query("/Users/demo/Documents report").unwrap();
    let Expr::And(parts) = query.expr else {
        panic!("expected conjunction");
    };
    assert_eq!(parts.len(), 2);

    let Expr::Term(Term::Word(path)) = &parts[0] else {
        panic!();
    };
    assert_eq!(path, "/Users/demo/Documents");
    assert_eq!(parts[1], word("report"));

    let query = parse_query("/Volumes/Data OR /Users").unwrap();
    let Expr::Or(options) = query.expr else {
        panic!("expected OR expression");
    };
    assert_eq!(options.len(), 2);
    assert!(matches!(
        &options[0],
        Expr::Term(Term::Word(path)) if path == "/Volumes/Data"
    ));
    assert!(matches!(
        &options[1],
        Expr::Term(Term::Word(path)) if path == "/Users"
    ));
}

#[test]
fn parses_grouping_expression() {
    let query = parse_query("<D: | E:> *.mp3").unwrap();
    let Expr::And(parts) = query.expr else {
        panic!("expected conjunction");
    };
    assert_eq!(parts.len(), 2);

    let Expr::Or(choices) = &parts[0] else {
        panic!("expected OR group");
    };
    assert_eq!(choices.len(), 2);
    let mut names = Vec::new();
    for choice in choices {
        let Expr::Term(Term::Filter(filter)) = choice else {
            panic!("expected filter term");
        };
        assert!(filter.argument.is_none());
        let FilterKind::Custom(name) = &filter.kind else {
            panic!("expected drive-like custom filter");
        };
        names.push(name.as_str());
    }
    assert_eq!(names, ["D", "E"]);

    assert_eq!(parts[1], word("*.mp3"));
}

#[test]
fn parses_parentheses_group() {
    let query = parse_query("(foo bar) baz").unwrap();
    let Expr::And(parts) = query.expr else {
        panic!("expected conjunction");
    };
    assert_eq!(
        parts,
        vec![Expr::And(vec![word("foo"), word("bar")]), word("baz")]
    );
}

#[test]
fn parses_and_with_leading_empty_operand() {
    let query = parse_query("  AND foo").unwrap();
    assert_eq!(query.expr, Expr::And(vec![Expr::Empty, word("foo")]));
}

#[test]
fn parses_and_with_trailing_empty_operand() {
    let query = parse_query("foo AND ").unwrap();
    assert_eq!(query.expr, Expr::And(vec![word("foo"), Expr::Empty]));
}

#[test]
fn parses_nested_groups() {
    let query = parse_query("(foo <bar|baz>) qux").unwrap();
    let Expr::And(parts) = query.expr else {
        panic!("expected conjunction");
    };
    assert_eq!(parts.len(), 2);

    let Expr::And(group_parts) = &parts[0] else {
        panic!("expected grouped conjunction");
    };
    assert_eq!(group_parts.len(), 2);
    assert_eq!(group_parts[0], word("foo"));

    let Expr::Or(region) = &group_parts[1] else {
        panic!("expected OR group");
    };
    assert_eq!(region.len(), 2);
    assert_eq!(region[0], word("bar"));
    assert_eq!(region[1], word("baz"));
    assert_eq!(parts[1], word("qux"));
}

#[test]
fn parses_or_with_trailing_empty_operand() {
    let query = parse_query("kksk | ").unwrap();
    assert_eq!(query.expr, Expr::Or(vec![word("kksk"), Expr::Empty]));
}

#[test]
fn parses_or_with_only_empty_operands() {
    let query = parse_query(" | ").unwrap();
    assert_eq!(query.expr, Expr::Or(vec![Expr::Empty, Expr::Empty]));
}

#[test]
fn parses_or_with_leading_empty_operand() {
    let query = parse_query("| foo").unwrap();
    assert_eq!(query.expr, Expr::Or(vec![Expr::Empty, word("foo")]));
}

#[test]
fn parses_and_with_only_empty_operands() {
    let query = parse_query(" AND ").unwrap();
    assert_eq!(query.expr, Expr::And(vec![Expr::Empty, Expr::Empty]));
}

#[test]
fn parses_or_with_consecutive_separators() {
    let query = parse_query("foo||bar").unwrap();
    assert_eq!(
        query.expr,
        Expr::Or(vec![word("foo"), Expr::Empty, word("bar")])
    );
}

#[test]
fn parses_or_with_empty_operands_on_both_sides() {
    let query = parse_query("| foo |").unwrap();
    assert_eq!(
        query.expr,
        Expr::Or(vec![Expr::Empty, word("foo"), Expr::Empty])
    );
}

#[test]
fn parses_filters_with_lists_and_ranges() {
    let query = parse_query("ext:txt;doc size:1mb..10mb").unwrap();
    let Expr::And(parts) = query.expr else {
        panic!("expected conjunction");
    };
    assert_eq!(parts.len(), 2);

    let Expr::Term(Term::Filter(ext)) = &parts[0] else {
        panic!("expected filter");
    };
    assert!(matches!(ext.kind, FilterKind::Ext));
    let ArgumentKind::List(values) = &ext.argument.as_ref().unwrap().kind else {
        panic!("expected list argument");
    };
    assert_eq!(values, &["txt", "doc"]);

    let Expr::Term(Term::Filter(size)) = &parts[1] else {
        panic!("expected filter");
    };
    let ArgumentKind::Range(range) = &size.argument.as_ref().unwrap().kind else {
        panic!("expected range argument");
    };
    assert_eq!(range.start.as_deref(), Some("1mb"));
    assert_eq!(range.end.as_deref(), Some("10mb"));
    assert!(matches!(range.separator, RangeSeparator::Dots));
}

#[test]
fn parses_date_range_with_hyphen() {
    let query = parse_query("dc:2014/8/1-2014/8/31").unwrap();
    let Expr::Term(Term::Filter(filter)) = query.expr else {
        panic!("expected filter term");
    };
    let ArgumentKind::Range(range) = filter.argument.unwrap().kind else {
        panic!("expected range");
    };
    assert_eq!(range.start.as_deref(), Some("2014/8/1"));
    assert_eq!(range.end.as_deref(), Some("2014/8/31"));
    assert!(matches!(range.separator, RangeSeparator::Hyphen));
}

#[test]
fn parses_comparison_arguments() {
    let query = parse_query("size:>1GB width:<=4000").unwrap();
    let Expr::And(parts) = query.expr else {
        panic!("expected AND expression");
    };
    let Expr::Term(Term::Filter(size)) = &parts[0] else {
        panic!();
    };
    let ArgumentKind::Comparison(ComparisonValue { op, value }) =
        &size.argument.as_ref().unwrap().kind
    else {
        panic!();
    };
    assert_eq!(*op, ComparisonOp::Gt);
    assert_eq!(value, "1GB");

    let Expr::Term(Term::Filter(width)) = &parts[1] else {
        panic!();
    };
    let ArgumentKind::Comparison(ComparisonValue { op, value }) =
        &width.argument.as_ref().unwrap().kind
    else {
        panic!();
    };
    assert_eq!(*op, ComparisonOp::Lte);
    assert_eq!(value, "4000");
}

#[test]
fn parses_file_filter_with_wildcard_and_date_keyword() {
    let query = parse_query("file: *.md dm:pastyear").unwrap();
    let Expr::And(parts) = query.expr else {
        panic!("expected conjunction");
    };
    assert_eq!(parts.len(), 3);

    let Expr::Term(Term::Filter(file_filter)) = &parts[0] else {
        panic!("expected filter term");
    };
    assert!(matches!(file_filter.kind, FilterKind::File));
    assert!(file_filter.argument.is_none());

    assert_eq!(parts[1], word("*.md"));

    let Expr::Term(Term::Filter(dm_filter)) = &parts[2] else {
        panic!("expected dm: filter");
    };
    assert!(matches!(dm_filter.kind, FilterKind::DateModified));
    let Some(argument) = dm_filter.argument.as_ref() else {
        panic!("expected dm: argument");
    };
    assert_eq!(argument.raw, "pastyear");
    assert!(matches!(argument.kind, ArgumentKind::Bare));
}

#[test]
fn parses_unc_paths_and_child_filter() {
    let query = parse_query("\\\\srv\\share child:*.mp3").unwrap();
    let Expr::And(parts) = query.expr else {
        panic!();
    };
    let Expr::Term(Term::Word(path)) = &parts[0] else {
        panic!();
    };
    assert_eq!(path, "\\\\srv\\share");

    let Expr::Term(Term::Filter(child)) = &parts[1] else {
        panic!();
    };
    assert!(matches!(child.kind, FilterKind::Child));
    assert_eq!(child.argument.as_ref().unwrap().raw, "*.mp3");
}

#[test]
fn parses_regex_prefix() {
    let query = parse_query(" regex:[^\\x00-\\x7f] ").unwrap();
    assert_eq!(query.expr, Expr::Term(Term::Regex("[^\\x00-\\x7f]".into())));
}

#[test]
fn reports_unmatched_groups() {
    let err = parse_query("<foo bar").unwrap_err();
    assert!(err.message.contains("expected '>'"));
}

#[derive(Debug)]
struct DocExample {
    line: usize,
    query: &'static str,
}

fn assert_doc_examples(examples: &[DocExample]) {
    for example in examples {
        if let Err(err) = parse_query(example.query) {
            panic!(
                "manual example at line {} failed to parse `{}`: {}",
                example.line, example.query, err
            );
        }
    }
}

#[test]
fn parses_all_manual_examples() {
    const EXAMPLES: &[DocExample] = &[
        DocExample {
            line: 9,
            query: "reports",
        },
        DocExample {
            line: 13,
            query: "D:",
        },
        DocExample {
            line: 13,
            query: r#"D:\Downloads\"#,
        },
        DocExample {
            line: 17,
            query: r#""C:\Program Files\\""#,
        },
        DocExample {
            line: 19,
            query: r#"documents\"#,
        },
        DocExample {
            line: 27,
            query: r#""summer holiday""#,
        },
        DocExample {
            line: 33,
            query: "foo bar",
        },
        DocExample {
            line: 37,
            query: "foo|bar",
        },
        DocExample {
            line: 41,
            query: "!temp",
        },
        DocExample {
            line: 47,
            query: "<D: | E:> *.mp3",
        },
        DocExample {
            line: 57,
            query: r#""Program Files""#,
        },
        DocExample {
            line: 59,
            query: "foo AND bar",
        },
        DocExample {
            line: 61,
            query: "NOT temp",
        },
        DocExample {
            line: 81,
            query: "*.mp3",
        },
        DocExample {
            line: 81,
            query: "report-??.txt",
        },
        DocExample {
            line: 87,
            query: "a*b",
        },
        DocExample {
            line: 102,
            query: "file: 报告",
        },
        DocExample {
            line: 102,
            query: "folder: 项目",
        },
        DocExample {
            line: 107,
            query: "ext:txt;doc;docx",
        },
        DocExample {
            line: 114,
            query: "type:picture",
        },
        DocExample {
            line: 114,
            query: "type:video",
        },
        DocExample {
            line: 114,
            query: "type:document",
        },
        DocExample {
            line: 114,
            query: "audio:",
        },
        DocExample {
            line: 114,
            query: "video:",
        },
        DocExample {
            line: 114,
            query: "doc:",
        },
        DocExample {
            line: 114,
            query: "exe:",
        },
        DocExample {
            line: 123,
            query: "size:>1GB",
        },
        DocExample {
            line: 152,
            query: "dm:today",
        },
        DocExample {
            line: 152,
            query: "dc:thisweek",
        },
        DocExample {
            line: 152,
            query: "dc:2023-01-01..2023-12-31",
        },
        DocExample {
            line: 170,
            query: r"parent:C:\Windows",
        },
        DocExample {
            line: 174,
            query: r"D:\Music\",
        },
        DocExample {
            line: 174,
            query: r"D:\Music\ !child:*.mp3",
        },
        DocExample {
            line: 183,
            query: "attrib:H",
        },
        DocExample {
            line: 183,
            query: "attrib:R",
        },
        DocExample {
            line: 250,
            query: "dimensions:1920x1080",
        },
        DocExample {
            line: 250,
            query: "width:>4000",
        },
        DocExample {
            line: 276,
            query: "dupe: *.mp4",
        },
        DocExample {
            line: 276,
            query: "size:>1gb sizedupe:",
        },
        DocExample {
            line: 284,
            query: "folder: dm:pastmonth ext:docx report",
        },
        DocExample {
            line: 296,
            query: r"regex:^Report.*2025$",
        },
        DocExample {
            line: 332,
            query: r"regex:^(?i)Readme\.",
        },
        DocExample {
            line: 332,
            query: r"regex:[0-9]{4}",
        },
        DocExample {
            line: 332,
            query: r"regex:\.jpe?g$",
        },
        DocExample {
            line: 337,
            query: r"regex:error",
        },
        DocExample {
            line: 337,
            query: "*.log",
        },
        DocExample {
            line: 344,
            query: "ABC 123",
        },
        DocExample {
            line: 349,
            query: "ABC|123",
        },
        DocExample {
            line: 359,
            query: "case:ABC",
        },
        DocExample {
            line: 367,
            query: "D:|E: *.mp3",
        },
        DocExample {
            line: 370,
            query: "D: *.jpg|*.png",
        },
        DocExample {
            line: 373,
            query: "!.",
        },
        DocExample {
            line: 378,
            query: "file:",
        },
        DocExample {
            line: 378,
            query: "folder:",
        },
        DocExample {
            line: 390,
            query: "size:>1mb",
        },
        DocExample {
            line: 390,
            query: "size:1mb..10mb",
        },
        DocExample {
            line: 396,
            query: "dm:today",
        },
        DocExample {
            line: 399,
            query: "dc:thisweek",
        },
        DocExample {
            line: 402,
            query: "dc:1/8/2014-31/8/2014",
        },
        DocExample {
            line: 402,
            query: "dc:2014/8/1-2014/8/31",
        },
        DocExample {
            line: 407,
            query: r"D:\music\ !child:*.mp3",
        },
        DocExample {
            line: 412,
            query: r"regex:[^\x00-\x7f]",
        },
        DocExample {
            line: 428,
            query: "video: size:>1gb",
        },
        DocExample {
            line: 439,
            query: "audio: year:2024",
        },
        DocExample {
            line: 439,
            query: "musiclastyear:",
        },
        DocExample {
            line: 478,
            query: r"%TEMP%\*.log",
        },
        DocExample {
            line: 486,
            query: "ext:pdf report",
        },
        DocExample {
            line: 501,
            query: "Pictures vacation",
        },
        DocExample {
            line: 501,
            query: "report ext:docx",
        },
    ];

    assert_doc_examples(EXAMPLES);
}
