//! 大小过滤器（size:）的解析与求值辅助。

use anyhow::{Result, anyhow, bail};
use cardinal_syntax::{ArgumentKind, ComparisonOp, FilterArgument, RangeSeparator};

pub(super) struct SizePredicate {
    kind: SizePredicateKind,
}

pub(super) enum SizePredicateKind {
    Comparison { op: ComparisonOp, value: u64 },
    Range { min: Option<u64>, max: Option<u64> },
}

impl SizePredicate {
    pub(super) fn parse(argument: &FilterArgument) -> Result<Self> {
        match &argument.kind {
            ArgumentKind::Comparison(comp) => {
                if size_keyword(&comp.value).is_some() {
                    bail!("size keywords cannot be used with comparison operators");
                }
                let value = parse_size_literal(&comp.value)?;
                Ok(SizePredicate {
                    kind: SizePredicateKind::Comparison { op: comp.op, value },
                })
            }
            ArgumentKind::Range(range) => {
                if range.separator != RangeSeparator::Dots {
                    bail!("size: only .. ranges are supported");
                }
                let start = match &range.start {
                    Some(value) => Some(parse_size_literal(value)?),
                    None => None,
                };
                let end = match &range.end {
                    Some(value) => Some(parse_size_literal(value)?),
                    None => None,
                };
                if let (Some(s), Some(e)) = (start, end)
                    && s > e
                {
                    bail!("size range start must be less than or equal to the end");
                }
                Ok(SizePredicate {
                    kind: SizePredicateKind::Range {
                        min: start,
                        max: end,
                    },
                })
            }
            ArgumentKind::List(_) => bail!("size: lists are not supported"),
            _ => SizePredicate::from_bare_value(&argument.raw),
        }
    }

    pub(super) fn from_bare_value(raw: &str) -> Result<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            bail!("size: requires a value");
        }
        if let Some(range) = size_keyword(trimmed) {
            return Ok(SizePredicate {
                kind: SizePredicateKind::Range {
                    min: range.min,
                    max: range.max,
                },
            });
        }
        let value = parse_size_literal(trimmed)?;
        Ok(SizePredicate {
            kind: SizePredicateKind::Comparison {
                op: ComparisonOp::Eq,
                value,
            },
        })
    }

    pub(super) fn matches(&self, size: u64) -> bool {
        match &self.kind {
            SizePredicateKind::Comparison { op, value } => match op {
                ComparisonOp::Lt => size < *value,
                ComparisonOp::Lte => size <= *value,
                ComparisonOp::Gt => size > *value,
                ComparisonOp::Gte => size >= *value,
                ComparisonOp::Eq => size == *value,
                ComparisonOp::Ne => size != *value,
            },
            SizePredicateKind::Range { min, max } => {
                if let Some(start) = min
                    && size < *start
                {
                    return false;
                }
                if let Some(end) = max
                    && size > *end
                {
                    return false;
                }
                true
            }
        }
    }
}

pub(super) struct SizeKeywordRange {
    min: Option<u64>,
    max: Option<u64>,
}

pub(super) fn size_keyword(name: &str) -> Option<SizeKeywordRange> {
    let normalized = name.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "empty" => Some(SizeKeywordRange {
            min: Some(0),
            max: Some(0),
        }),
        "tiny" => Some(SizeKeywordRange {
            min: Some(0),
            max: Some(10 * KB),
        }),
        "small" => Some(SizeKeywordRange {
            min: Some(10 * KB + 1),
            max: Some(100 * KB),
        }),
        "medium" => Some(SizeKeywordRange {
            min: Some(100 * KB + 1),
            max: Some(MB),
        }),
        "large" => Some(SizeKeywordRange {
            min: Some(MB + 1),
            max: Some(16 * MB),
        }),
        "huge" => Some(SizeKeywordRange {
            min: Some(16 * MB + 1),
            max: Some(128 * MB),
        }),
        "gigantic" | "giant" => Some(SizeKeywordRange {
            min: Some(128 * MB + 1),
            max: None,
        }),
        _ => None,
    }
}

pub(super) const KB: u64 = 1024;
pub(super) const MB: u64 = 1024 * 1024;

pub(super) fn parse_size_literal(raw: &str) -> Result<u64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("size: expected a number");
    }
    let mut split = trimmed.len();
    for (idx, ch) in trimmed.char_indices() {
        if ch.is_ascii_digit() || ch == '.' {
            continue;
        }
        split = idx;
        break;
    }

    let (value_part, unit_part) = trimmed.split_at(split);
    if value_part.is_empty() {
        bail!("size: expected a numeric value in {raw:?}");
    }
    let value: f64 = value_part
        .parse()
        .map_err(|_| anyhow!("size: failed to parse number in {raw:?}"))?;
    let multiplier = size_unit_multiplier(unit_part)?;
    let bytes = (value * multiplier as f64).round();
    if !bytes.is_finite() || bytes < 0.0 {
        bail!("size: value {raw:?} is out of range");
    }
    if bytes > u64::MAX as f64 {
        Ok(u64::MAX)
    } else {
        Ok(bytes as u64)
    }
}

pub(super) fn size_unit_multiplier(unit: &str) -> Result<u64> {
    let normalized = unit.trim().to_ascii_lowercase();
    let multiplier = match normalized.as_str() {
        "" | "b" | "byte" | "bytes" => 1,
        "k" | "kb" | "kib" | "kilobyte" | "kilobytes" => 1024,
        "m" | "mb" | "mib" | "megabyte" | "megabytes" => 1024 * 1024,
        "g" | "gb" | "gib" | "gigabyte" | "gigabytes" => 1024 * 1024 * 1024,
        "t" | "tb" | "tib" | "terabyte" | "terabytes" => 1024_u64.pow(4),
        "p" | "pb" | "pib" | "petabyte" | "petabytes" => 1024_u64.pow(5),
        _ => bail!("Unknown size unit: {unit:?}"),
    };
    Ok(multiplier)
}
