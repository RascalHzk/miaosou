//! 日期过滤器（dm/dc/da/dr）的解析与求值辅助。

use anyhow::{Result, bail};
use cardinal_syntax::{ArgumentKind, ComparisonOp, FilterArgument};
use jiff::{Timestamp, civil::Date, tz::TimeZone};

#[derive(Clone, Copy)]
pub(super) enum DateField {
    Modified,
    Created,
}

pub(super) struct DateContext {
    tz: TimeZone,
    today: Date,
}

impl DateContext {
    pub(super) fn capture() -> Self {
        let tz = TimeZone::system();
        let zoned = Timestamp::now().to_zoned(tz.clone());
        Self {
            tz,
            today: zoned.date(),
        }
    }
}

pub(super) struct DatePredicate {
    kind: DatePredicateKind,
}

#[derive(Clone, Copy)]
pub(super) enum DatePredicateKind {
    Range {
        start: Option<i64>,
        end: Option<i64>,
    },
    NotEqual {
        start: i64,
        end: i64,
    },
}

impl DatePredicate {
    pub(super) fn parse(argument: &FilterArgument, context: &DateContext) -> Result<Self> {
        match &argument.kind {
            ArgumentKind::Range(range) => {
                let start = match &range.start {
                    Some(value) => Some(parse_date_value(value, context)?.start),
                    None => None,
                };
                let end = match &range.end {
                    Some(value) => Some(parse_date_value(value, context)?.end),
                    None => None,
                };
                if let (Some(s), Some(e)) = (start, end)
                    && s > e
                {
                    bail!("date range start must not exceed end");
                }
                Ok(Self {
                    kind: DatePredicateKind::Range { start, end },
                })
            }
            ArgumentKind::Comparison(comp) => {
                let value = parse_date_value(&comp.value, context)?;
                let predicate = match comp.op {
                    ComparisonOp::Lt => {
                        let bound = value.start.saturating_sub(1);
                        DatePredicate::range(None, Some(bound))
                    }
                    ComparisonOp::Lte => DatePredicate::range(None, Some(value.end)),
                    ComparisonOp::Gt => DatePredicate::range(Some(value.end + 1), None),
                    ComparisonOp::Gte => DatePredicate::range(Some(value.start), None),
                    ComparisonOp::Eq => DatePredicate::range(Some(value.start), Some(value.end)),
                    ComparisonOp::Ne => DatePredicate {
                        kind: DatePredicateKind::NotEqual {
                            start: value.start,
                            end: value.end,
                        },
                    },
                };
                Ok(predicate)
            }
            ArgumentKind::Phrase | ArgumentKind::Bare => {
                let value = parse_date_value(&argument.raw, context)?;
                Ok(DatePredicate::range(Some(value.start), Some(value.end)))
            }
            ArgumentKind::List(_) => bail!("date filters do not accept lists"),
        }
    }

    pub(super) fn range(start: Option<i64>, end: Option<i64>) -> Self {
        Self {
            kind: DatePredicateKind::Range { start, end },
        }
    }

    pub(super) fn matches(&self, timestamp: i64) -> bool {
        match self.kind {
            DatePredicateKind::Range { start, end } => {
                if let Some(bound) = start
                    && timestamp < bound
                {
                    return false;
                }
                if let Some(bound) = end
                    && timestamp > bound
                {
                    return false;
                }
                true
            }
            DatePredicateKind::NotEqual { start, end } => timestamp < start || timestamp > end,
        }
    }
}

pub(super) struct DateValue {
    start: i64,
    end: i64,
}

pub(super) fn parse_date_value(raw: &str, context: &DateContext) -> Result<DateValue> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("date filters require a value");
    }
    if let Some(range) = keyword_range(trimmed, context) {
        return Ok(range);
    }
    if let Some(date) = parse_absolute_date(trimmed) {
        if let Some(bounds) = day_bounds(date, context) {
            return Ok(DateValue {
                start: bounds.0,
                end: bounds.1,
            });
        } else {
            bail!("Date {trimmed:?} is out of range");
        }
    }
    bail!("Unrecognized date literal: {trimmed}");
}

pub(super) fn keyword_range(keyword: &str, context: &DateContext) -> Option<DateValue> {
    let lower = keyword.to_ascii_lowercase();
    let today = context.today;
    let year = today.year();
    let month = today.month();
    match lower.as_str() {
        "today" => day_bounds(today, context).map(|(s, e)| DateValue { start: s, end: e }),
        "yesterday" => {
            let date = shift_days(today, -1)?;
            day_bounds(date, context).map(|(s, e)| DateValue { start: s, end: e })
        }
        "thisweek" => {
            let weekday_offset = i64::from(today.weekday().to_monday_zero_offset());
            let start = shift_days(today, -weekday_offset)?;
            let end = shift_days(start, 6)?;
            range_from_dates(start, end, context)
        }
        "lastweek" => {
            let weekday_offset = i64::from(today.weekday().to_monday_zero_offset()) + 7;
            let start = shift_days(today, -weekday_offset)?;
            let end = shift_days(start, 6)?;
            range_from_dates(start, end, context)
        }
        "thismonth" => month_range(year, month, context),
        "lastmonth" => {
            let (year, month) = if month == 1 {
                (year.checked_sub(1)?, 12)
            } else {
                (year, month - 1)
            };
            month_range(year, month, context)
        }
        "thisyear" => year_range(year, context),
        "lastyear" => year_range(year.checked_sub(1)?, context),
        "pastweek" => trailing_range(context, 7),
        "pastmonth" => trailing_range(context, 30),
        "pastyear" => trailing_range(context, 365),
        _ => None,
    }
}

pub(super) fn trailing_range(context: &DateContext, days: i64) -> Option<DateValue> {
    let start_date = shift_days(context.today, -days)?;
    range_from_dates(start_date, context.today, context)
}

pub(super) fn month_range(year: i16, month: i8, context: &DateContext) -> Option<DateValue> {
    let start = Date::new(year, month, 1).ok()?;
    let (next_year, next_month) = if month == 12 {
        (year.checked_add(1)?, 1)
    } else {
        (year, month + 1)
    };
    let next_start = Date::new(next_year, next_month, 1).ok()?;
    let end = next_start.yesterday().ok()?;
    range_from_dates(start, end, context)
}

pub(super) fn year_range(year: i16, context: &DateContext) -> Option<DateValue> {
    let start = Date::new(year, 1, 1).ok()?;
    let end = Date::new(year, 12, 31).ok()?;
    range_from_dates(start, end, context)
}

pub(super) fn range_from_dates(start: Date, end: Date, context: &DateContext) -> Option<DateValue> {
    if end < start {
        return None;
    }
    let (start_ts, _) = day_bounds(start, context)?;
    let (_, end_ts) = day_bounds(end, context)?;
    Some(DateValue {
        start: start_ts,
        end: end_ts,
    })
}

pub(super) fn shift_days(date: Date, delta: i64) -> Option<Date> {
    if delta == 0 {
        return Some(date);
    }
    let mut current = date;
    if delta > 0 {
        let steps = delta.unsigned_abs() as usize;
        for _ in 0..steps {
            current = current.tomorrow().ok()?;
        }
    } else {
        let steps = (-delta).unsigned_abs() as usize;
        for _ in 0..steps {
            current = current.yesterday().ok()?;
        }
    }
    Some(current)
}

pub(super) fn day_bounds(date: Date, context: &DateContext) -> Option<(i64, i64)> {
    let start = context
        .tz
        .to_zoned(date.at(0, 0, 0, 0))
        .ok()?
        .timestamp()
        .as_second();
    let next_day = date.tomorrow().ok()?;
    let next_start = context
        .tz
        .to_zoned(next_day.at(0, 0, 0, 0))
        .ok()?
        .timestamp()
        .as_second();
    let end = next_start.checked_sub(1)?;
    Some((start, end))
}

pub(super) fn parse_absolute_date(raw: &str) -> Option<Date> {
    let trimmed = raw.trim();
    let sep = trimmed.chars().find(|ch| matches!(ch, '-' | '/' | '.'))?;
    let mut formats = match sep {
        '-' => vec!["%Y-%m-%d", "%d-%m-%Y", "%m-%d-%Y"],
        '/' => vec!["%Y/%m/%d", "%m/%d/%Y", "%d/%m/%Y"],
        '.' => vec!["%Y.%m.%d", "%d.%m.%Y", "%m.%d.%Y"],
        _ => vec![],
    };
    let starts_with_year = trimmed.len() >= 4
        && trimmed.chars().take(4).all(|c| c.is_ascii_digit())
        && matches!(trimmed.chars().nth(4), Some('-' | '/' | '.'));
    formats.sort_by_key(|fmt| {
        let year_first = fmt.starts_with("%Y");
        if starts_with_year {
            if year_first { 0 } else { 1 }
        } else if year_first {
            1
        } else {
            0
        }
    });
    for fmt in formats {
        if let Ok(date) = Date::strptime(fmt, trimmed) {
            return Some(date);
        }
    }
    None
}
