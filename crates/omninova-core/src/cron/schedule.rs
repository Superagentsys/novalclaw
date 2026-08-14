//! Schedule parsing and next-occurrence computation for automation jobs.
//!
//! Two syntaxes are accepted:
//!
//! * interval — `every 45s`, `every 30m`, `every 2h`, `every 1d`, or a bare
//!   number of seconds.
//! * cron — a five field expression `minute hour day-of-month month day-of-week`
//!   supporting `*`, `*/step`, `a-b`, `a-b/step` and comma separated lists.
//!
//! Cron expressions are evaluated against a fixed UTC offset carried by the job
//! rather than the host timezone: the desktop client knows the user's offset and
//! the gateway may run headless in a different one, so pinning it per job keeps
//! "every day at 09:00" stable no matter who evaluates it.

use anyhow::{bail, Context, Result};
use time::{Duration, OffsetDateTime, UtcOffset};

/// Upper bound for the minute-by-minute scan; a year covers every expression
/// that can ever match, and impossible ones (e.g. Feb 30) terminate instead of
/// looping forever.
const MAX_LOOKAHEAD_MINUTES: u32 = 366 * 24 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Schedule {
    /// Fire every N seconds, measured from the previous fire.
    Interval(i64),
    Cron(CronExpr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronExpr {
    minutes: Vec<u8>,
    hours: Vec<u8>,
    days_of_month: Vec<u8>,
    months: Vec<u8>,
    days_of_week: Vec<u8>,
    dom_restricted: bool,
    dow_restricted: bool,
}

impl Schedule {
    pub fn parse(spec: &str) -> Result<Self> {
        let trimmed = spec.trim();
        if trimmed.is_empty() {
            bail!("schedule is empty");
        }
        if let Some(seconds) = parse_interval_secs(trimmed) {
            if seconds <= 0 {
                bail!("interval must be greater than zero");
            }
            return Ok(Self::Interval(seconds));
        }
        Ok(Self::Cron(CronExpr::parse(trimmed)?))
    }

    /// First occurrence strictly after `after`, returned in UTC.
    pub fn next_after(&self, after: OffsetDateTime, tz: UtcOffset) -> Option<OffsetDateTime> {
        match self {
            Self::Interval(seconds) => Some(after + Duration::seconds(*seconds)),
            Self::Cron(expr) => expr.next_after(after, tz),
        }
    }

    /// RFC 3339 timestamp of the next fire after now, using `tz_offset_minutes`.
    pub fn next_run_iso(&self, tz_offset_minutes: i32) -> Option<String> {
        self.next_after(OffsetDateTime::now_utc(), offset_from_minutes(tz_offset_minutes))
            .and_then(|dt| {
                dt.format(&time::format_description::well_known::Rfc3339)
                    .ok()
            })
    }
}

impl CronExpr {
    pub fn parse(spec: &str) -> Result<Self> {
        let fields: Vec<&str> = spec.split_whitespace().collect();
        if fields.len() != 5 {
            bail!(
                "cron expression needs 5 fields (minute hour day-of-month month day-of-week), got {}",
                fields.len()
            );
        }

        let (minutes, _) = parse_field(fields[0], 0, 59, "minute")?;
        let (hours, _) = parse_field(fields[1], 0, 23, "hour")?;
        let (days_of_month, dom_restricted) = parse_field(fields[2], 1, 31, "day-of-month")?;
        let (months, _) = parse_field(fields[3], 1, 12, "month")?;
        let (days_of_week, dow_restricted) = parse_field(fields[4], 0, 7, "day-of-week")?;

        // Cron allows both 0 and 7 for Sunday; collapse to 0 so lookups are simple.
        let mut days_of_week: Vec<u8> = days_of_week
            .into_iter()
            .map(|value| if value == 7 { 0 } else { value })
            .collect();
        days_of_week.sort_unstable();
        days_of_week.dedup();

        Ok(Self {
            minutes,
            hours,
            days_of_month,
            months,
            days_of_week,
            dom_restricted,
            dow_restricted,
        })
    }

    fn next_after(&self, after: OffsetDateTime, tz: UtcOffset) -> Option<OffsetDateTime> {
        let local = after.to_offset(tz);
        let mut cursor = local
            .replace_second(0)
            .ok()?
            .replace_nanosecond(0)
            .ok()?
            .checked_add(Duration::minutes(1))?;

        for _ in 0..MAX_LOOKAHEAD_MINUTES {
            if self.matches(cursor) {
                return Some(cursor.to_offset(UtcOffset::UTC));
            }
            cursor = cursor.checked_add(Duration::minutes(1))?;
        }
        None
    }

    fn matches(&self, dt: OffsetDateTime) -> bool {
        if !self.minutes.contains(&dt.minute()) {
            return false;
        }
        if !self.hours.contains(&dt.hour()) {
            return false;
        }
        if !self.months.contains(&u8::from(dt.month())) {
            return false;
        }

        let day_of_month = dt.day();
        let day_of_week = dt.weekday().number_days_from_sunday();
        // Standard cron: when both day fields are restricted they are OR-ed.
        match (self.dom_restricted, self.dow_restricted) {
            (true, true) => {
                self.days_of_month.contains(&day_of_month)
                    || self.days_of_week.contains(&day_of_week)
            }
            (true, false) => self.days_of_month.contains(&day_of_month),
            (false, true) => self.days_of_week.contains(&day_of_week),
            (false, false) => true,
        }
    }
}

/// Parses one cron field, returning its allowed values and whether the field
/// was restricted (anything other than a bare `*`).
fn parse_field(raw: &str, min: u8, max: u8, label: &str) -> Result<(Vec<u8>, bool)> {
    let raw = raw.trim();
    if raw.is_empty() {
        bail!("{label} field is empty");
    }
    if raw == "*" {
        return Ok(((min..=max).collect(), false));
    }

    let mut values: Vec<u8> = Vec::new();
    for entry in raw.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            bail!("{label} field has an empty list entry");
        }

        let (range_part, step) = match entry.split_once('/') {
            Some((range, step)) => {
                let step: u8 = step
                    .trim()
                    .parse()
                    .with_context(|| format!("invalid step in {label} field: {entry}"))?;
                if step == 0 {
                    bail!("{label} field step must be greater than zero");
                }
                (range.trim(), step)
            }
            None => (entry, 1),
        };

        let (start, end) = if range_part == "*" {
            (min, max)
        } else if let Some((from, to)) = range_part.split_once('-') {
            let from: u8 = from
                .trim()
                .parse()
                .with_context(|| format!("invalid range in {label} field: {entry}"))?;
            let to: u8 = to
                .trim()
                .parse()
                .with_context(|| format!("invalid range in {label} field: {entry}"))?;
            (from, to)
        } else {
            let value: u8 = range_part
                .trim()
                .parse()
                .with_context(|| format!("invalid value in {label} field: {entry}"))?;
            // `5/10` means "from 5 to the end of the field, every 10".
            if step > 1 { (value, max) } else { (value, value) }
        };

        if start < min || end > max || start > end {
            bail!("{label} field value out of range ({min}-{max}): {entry}");
        }

        let mut value = start;
        while value <= end {
            values.push(value);
            match value.checked_add(step) {
                Some(next) => value = next,
                None => break,
            }
        }
    }

    values.sort_unstable();
    values.dedup();
    if values.is_empty() {
        bail!("{label} field matches no value");
    }
    Ok((values, true))
}

fn parse_interval_secs(schedule: &str) -> Option<i64> {
    let spec = schedule.trim().to_lowercase();
    if let Ok(seconds) = spec.parse::<i64>() {
        return Some(seconds);
    }
    let rest = spec.strip_prefix("every ")?.trim();
    // Longest unit first so "hours" is not claimed by the trailing "s".
    const UNITS: &[(&str, i64)] = &[
        ("seconds", 1),
        ("second", 1),
        ("secs", 1),
        ("sec", 1),
        ("minutes", 60),
        ("minute", 60),
        ("mins", 60),
        ("min", 60),
        ("hours", 3600),
        ("hour", 3600),
        ("hrs", 3600),
        ("hr", 3600),
        ("days", 86_400),
        ("day", 86_400),
        ("s", 1),
        ("m", 60),
        ("h", 3600),
        ("d", 86_400),
    ];
    for (unit, multiplier) in UNITS {
        let Some(number) = rest.strip_suffix(unit).map(str::trim) else {
            continue;
        };
        if let Ok(value) = number.parse::<i64>() {
            return Some(value * multiplier);
        }
    }
    None
}

/// Builds a `UtcOffset` from minutes east of UTC, falling back to UTC when the
/// stored value is out of range.
pub fn offset_from_minutes(minutes: i32) -> UtcOffset {
    UtcOffset::from_whole_seconds(minutes * 60).unwrap_or(UtcOffset::UTC)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn parses_interval_forms() {
        assert_eq!(Schedule::parse("every 30s").unwrap(), Schedule::Interval(30));
        assert_eq!(Schedule::parse("every 5m").unwrap(), Schedule::Interval(300));
        assert_eq!(Schedule::parse("every 2 hours").unwrap(), Schedule::Interval(7200));
        assert_eq!(Schedule::parse("every 1d").unwrap(), Schedule::Interval(86_400));
        assert_eq!(Schedule::parse("90").unwrap(), Schedule::Interval(90));
    }

    #[test]
    fn daily_expression_respects_job_offset() {
        let schedule = Schedule::parse("0 9 * * *").unwrap();
        // 2026-08-13 02:00 UTC is 10:00 in UTC+8, so the next 09:00 local is the
        // following day, i.e. 2026-08-14 01:00 UTC.
        let now = datetime!(2026-08-13 02:00:00 UTC);
        let next = schedule.next_after(now, offset_from_minutes(480)).unwrap();
        assert_eq!(next, datetime!(2026-08-14 01:00:00 UTC));
    }

    #[test]
    fn weekday_expression_matches_next_monday() {
        let schedule = Schedule::parse("30 8 * * 1").unwrap();
        // Thursday 2026-08-13 in UTC -> next Monday is 2026-08-17.
        let now = datetime!(2026-08-13 12:00:00 UTC);
        let next = schedule.next_after(now, UtcOffset::UTC).unwrap();
        assert_eq!(next, datetime!(2026-08-17 08:30:00 UTC));
    }

    #[test]
    fn step_and_list_fields() {
        let schedule = Schedule::parse("*/15 * * * *").unwrap();
        let now = datetime!(2026-08-13 10:07:00 UTC);
        let next = schedule.next_after(now, UtcOffset::UTC).unwrap();
        assert_eq!(next, datetime!(2026-08-13 10:15:00 UTC));

        let schedule = Schedule::parse("0 9,18 * * *").unwrap();
        let now = datetime!(2026-08-13 10:00:00 UTC);
        let next = schedule.next_after(now, UtcOffset::UTC).unwrap();
        assert_eq!(next, datetime!(2026-08-13 18:00:00 UTC));
    }

    #[test]
    fn rejects_malformed_expressions() {
        assert!(Schedule::parse("* * *").is_err());
        assert!(Schedule::parse("99 * * * *").is_err());
        assert!(Schedule::parse("*/0 * * * *").is_err());
    }
}
