//! Five-field cron expressions (`minute hour day-of-month month day-of-week`), read in a time zone.

use anyhow::{Context, Result, bail, ensure};
use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;

pub const MINUTE: usize = 0;
pub const HOUR: usize = 1;
pub const DAY: usize = 2;
pub const MONTH: usize = 3;
pub const WEEKDAY: usize = 4;

const RANGES: [(u8, u8); 5] = [(0, 59), (0, 23), (1, 31), (1, 12), (0, 7)];
const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];
const WEEKDAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];
/// Far enough for a February 29th, which can be eight years away.
const MAX_DAYS: usize = 9 * 366;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cron {
    /// One bit per allowed value; Sunday is 0 for the day of the week.
    fields: [u64; 5],
    /// Day fields written with `*` (even `*/2`): as in Vixie cron, a day must then match both day fields,
    /// otherwise either.
    day_star: bool,
    weekday_star: bool,
}

impl Cron {
    pub fn parse(expr: &str) -> Result<Cron> {
        let parts: Vec<&str> = expr.split_whitespace().collect();
        ensure!(
            parts.len() == 5,
            "a schedule has 5 fields (minute hour day month weekday), not {}",
            parts.len()
        );
        let mut fields = [0; 5];
        for (i, part) in parts.iter().enumerate() {
            fields[i] = field(part, i).with_context(|| format!("invalid field {part:?}"))?;
        }
        // Sunday may be written 0 or 7.
        if fields[WEEKDAY] & 1 << 7 != 0 {
            fields[WEEKDAY] = fields[WEEKDAY] & !(1 << 7) | 1;
        }
        Ok(Cron {
            fields,
            day_star: parts[DAY].starts_with('*'),
            weekday_star: parts[WEEKDAY].starts_with('*'),
        })
    }

    pub fn values(&self, field: usize) -> impl Iterator<Item = u8> + '_ {
        (0..64u8).filter(move |v| self.fields[field] & 1 << v != 0)
    }

    /// Every value of the field is allowed.
    pub fn is_full(&self, field: usize) -> bool {
        let (lo, hi) = RANGES[field];
        let hi = if field == WEEKDAY { 6 } else { hi };
        (lo..=hi).all(|v| self.fields[field] & 1 << v != 0)
    }

    /// Whether a day runs when one of the day fields matches (both are restricted), rather than both.
    pub fn days_either(&self) -> bool {
        !self.day_star && !self.weekday_star
    }

    fn has(&self, field: usize, value: i8) -> bool {
        self.fields[field] & 1 << value != 0
    }

    fn runs_on(&self, date: Date) -> bool {
        if !self.has(MONTH, date.month()) {
            return false;
        }
        let day = self.has(DAY, date.day());
        let weekday = self.has(WEEKDAY, date.weekday().to_sunday_zero_offset());
        if self.days_either() {
            day || weekday
        } else {
            day && weekday
        }
    }

    /// The day's runs. A time the clocks skip (spring forward) runs as many minutes later as were skipped; a
    /// time they repeat (fall back) runs once, the first time.
    fn day_runs(&self, date: Date, tz: &TimeZone) -> Vec<Timestamp> {
        if !self.runs_on(date) {
            return Vec::new();
        }
        let mut runs = Vec::new();
        for hour in self.values(HOUR) {
            for minute in self.values(MINUTE) {
                if let Ok(t) = tz.to_timestamp(date.at(hour as i8, minute as i8, 0, 0)) {
                    runs.push(t);
                }
            }
        }
        runs
    }

    /// The runs `keep` accepts, from the day of `from` on, forward or backward. A shift in the clocks can move a
    /// day's runs past another day's, so the search reads the days on both sides of the first one that has some.
    fn search(
        &self,
        tz: &TimeZone,
        from: Timestamp,
        forward: bool,
        keep: impl Fn(Timestamp) -> bool,
    ) -> Vec<Timestamp> {
        let step = |d: Date| if forward { d.tomorrow() } else { d.yesterday() };
        let back = |d: Date| if forward { d.yesterday() } else { d.tomorrow() };
        let Ok(mut date) = back(tz.to_datetime(from).date()) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        let mut last = MAX_DAYS;
        for i in 0..MAX_DAYS {
            found.extend(self.day_runs(date, tz).into_iter().filter(|t| keep(*t)));
            if last == MAX_DAYS && !found.is_empty() {
                last = i + 1;
            }
            if i >= last {
                break;
            }
            let Ok(next) = step(date) else { break };
            date = next;
        }
        found
    }

    /// The first run strictly after `after`.
    pub fn next_after(&self, after: Timestamp, tz: &TimeZone) -> Option<Timestamp> {
        self.search(tz, after, true, |t| t > after)
            .into_iter()
            .min()
    }

    /// The last run at or before `until`.
    pub fn last_until(&self, until: Timestamp, tz: &TimeZone) -> Option<Timestamp> {
        self.search(tz, until, false, |t| t <= until)
            .into_iter()
            .max()
    }

    /// The next `count` runs after `after`.
    pub fn upcoming(&self, after: Timestamp, tz: &TimeZone, count: usize) -> Vec<Timestamp> {
        let mut runs = Vec::new();
        let mut from = after;
        while runs.len() < count
            && let Some(next) = self.next_after(from, tz)
        {
            runs.push(next);
            from = next;
        }
        runs
    }
}

fn field(text: &str, index: usize) -> Result<u64> {
    let (lo, hi) = RANGES[index];
    let mut bits = 0;
    for part in text.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((range, step)) => (range, Some(step.parse::<u8>().context("bad step")?)),
            None => (part, None),
        };
        ensure!(step != Some(0), "a step cannot be 0");
        let (start, end) = match range {
            "*" => (lo, hi),
            _ => match range.split_once('-') {
                Some((a, b)) => (value(a, index)?, value(b, index)?),
                // `5/15` goes from 5 to the end, as in Vixie cron.
                None if step.is_some() => (value(range, index)?, hi),
                None => {
                    let v = value(range, index)?;
                    (v, v)
                }
            },
        };
        ensure!(start <= end, "a range must go up ({start}-{end})");
        let step = step.unwrap_or(1) as usize;
        for v in (start..=end).step_by(step) {
            bits |= 1 << v;
        }
    }
    Ok(bits)
}

fn value(text: &str, index: usize) -> Result<u8> {
    let names: &[&str] = match index {
        MONTH => &MONTHS,
        WEEKDAY => &WEEKDAYS,
        _ => &[],
    };
    let lower = text.to_ascii_lowercase();
    let v = match names.iter().position(|n| *n == lower) {
        Some(i) if index == MONTH => i as u8 + 1,
        Some(i) => i as u8,
        None => text
            .parse()
            .with_context(|| format!("{text:?} is not a number"))?,
    };
    let (lo, hi) = RANGES[index];
    if !(lo..=hi).contains(&v) {
        bail!("{v} is out of {lo}-{hi}");
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paris() -> TimeZone {
        // A POSIX rule rather than the system's time zone database, so the test runs the same everywhere.
        TimeZone::posix("CET-1CEST,M3.5.0,M10.5.0/3").unwrap()
    }

    fn at(text: &str) -> Timestamp {
        text.parse().unwrap()
    }

    fn next(expr: &str, after: &str, tz: &TimeZone) -> String {
        let t = Cron::parse(expr)
            .unwrap()
            .next_after(at(after), tz)
            .unwrap();
        tz.to_datetime(t).to_string()
    }

    #[test]
    fn parses_fields() {
        let c = Cron::parse("*/15 9-17 1,15 jan-mar MON-fri").unwrap();
        assert_eq!(c.values(MINUTE).collect::<Vec<_>>(), [0, 15, 30, 45]);
        assert_eq!(c.values(HOUR).count(), 9);
        assert_eq!(c.values(DAY).collect::<Vec<_>>(), [1, 15]);
        assert_eq!(c.values(MONTH).collect::<Vec<_>>(), [1, 2, 3]);
        assert_eq!(c.values(WEEKDAY).collect::<Vec<_>>(), [1, 2, 3, 4, 5]);
        assert!(c.days_either());

        let c = Cron::parse("5/20 * * * 7").unwrap();
        assert_eq!(c.values(MINUTE).collect::<Vec<_>>(), [5, 25, 45]);
        assert_eq!(c.values(WEEKDAY).collect::<Vec<_>>(), [0], "7 is Sunday");
        assert!(c.is_full(HOUR) && !c.is_full(WEEKDAY) && !c.days_either());
        assert!(Cron::parse("* * * * 0-7").unwrap().is_full(WEEKDAY));

        for bad in [
            "* * * *",
            "* * * * * *",
            "60 * * * *",
            "* 24 * * *",
            "* * 0 * *",
            "* * * 13 *",
            "* * * * 8",
            "*/0 * * * *",
            "5-1 * * * *",
            "a * * * *",
            "1,,2 * * * *",
        ] {
            assert!(Cron::parse(bad).is_err(), "{bad} must be refused");
        }
    }

    #[test]
    fn finds_the_next_runs() {
        let utc = TimeZone::UTC;
        assert_eq!(
            next("0 9 * * 1-5", "2026-11-20T10:00:00Z", &utc),
            "2026-11-23T09:00:00",
            "Friday after 9 → Monday"
        );
        assert_eq!(
            next("*/15 * * * *", "2026-11-20T10:00:00Z", &utc),
            "2026-11-20T10:15:00",
            "strictly after"
        );
        assert_eq!(
            next("30 23 31 12 *", "2026-11-20T10:00:00Z", &utc),
            "2026-12-31T23:30:00"
        );
        let c = Cron::parse("0 9 * * 1").unwrap();
        let runs = c.upcoming(at("2026-11-20T10:00:00Z"), &utc, 3);
        let days: Vec<String> = runs
            .iter()
            .map(|t| utc.to_datetime(*t).date().to_string())
            .collect();
        assert_eq!(days, ["2026-11-23", "2026-11-30", "2026-12-07"]);
        assert_eq!(
            c.last_until(at("2026-11-23T09:00:00Z"), &utc),
            Some(at("2026-11-23T09:00:00Z")),
            "a run at the very time counts"
        );
        assert_eq!(
            c.last_until(at("2026-11-23T08:59:00Z"), &utc),
            Some(at("2026-11-16T09:00:00Z"))
        );
    }

    #[test]
    fn handles_month_ends_and_leap_years() {
        let utc = TimeZone::UTC;
        assert_eq!(
            next("0 0 31 * *", "2026-04-01T00:00:00Z", &utc),
            "2026-05-31T00:00:00",
            "April has no 31st"
        );
        assert_eq!(
            next("0 12 29 2 *", "2026-03-01T00:00:00Z", &utc),
            "2028-02-29T12:00:00"
        );
        assert_eq!(
            next("0 0 28-31 * *", "2026-02-27T00:00:00Z", &utc),
            "2026-02-28T00:00:00"
        );
        assert!(
            Cron::parse("0 0 30 2 *")
                .unwrap()
                .next_after(at("2026-01-01T00:00:00Z"), &utc)
                .is_none(),
            "February 30th never comes"
        );
    }

    /// xorshift64 with a fixed seed: the same cases on every run.
    struct Rng(u64);

    impl Rng {
        fn below(&mut self, n: u64) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0 % n
        }
    }

    /// A random field in `lo..=hi` and the values it allows (7 is Sunday, 0).
    fn random_field(rng: &mut Rng, lo: u64, hi: u64) -> (String, [bool; 64]) {
        let mut set = [false; 64];
        let mut add = |a: u64, b: u64, step: u64| {
            for v in (a..=b).step_by(step as usize) {
                set[if v == 7 && hi == 7 { 0 } else { v as usize }] = true;
            }
        };
        let pick = |rng: &mut Rng| lo + rng.below(hi - lo + 1);
        let text = match rng.below(6) {
            0 => {
                add(lo, hi, 1);
                "*".into()
            }
            1 => {
                let step = 1 + rng.below(hi - lo + 1);
                add(lo, hi, step);
                format!("*/{step}")
            }
            2 => {
                let v = pick(rng);
                add(v, v, 1);
                v.to_string()
            }
            3 => {
                let (a, b) = (pick(rng), pick(rng));
                let (a, b) = (a.min(b), a.max(b));
                let step = 1 + rng.below(4);
                add(a, b, step);
                format!("{a}-{b}/{step}")
            }
            4 => {
                let (a, b) = (pick(rng), pick(rng));
                add(a, a, 1);
                add(b, b, 1);
                format!("{a},{b}")
            }
            _ => {
                let a = pick(rng);
                let step = 1 + rng.below(9);
                add(a, hi, step);
                format!("{a}/{step}")
            }
        };
        (text, set)
    }

    #[test]
    fn next_runs_match_a_day_by_day_reference() {
        let mut rng = Rng(0x7e3f_1a2b_9c4d_5e6f);
        let ranges = [(0, 59), (0, 23), (1, 31), (1, 12), (0, 7)];
        for case in 0..400 {
            let fields: Vec<_> = ranges
                .iter()
                .map(|&(lo, hi)| random_field(&mut rng, lo, hi))
                .collect();
            let expr = fields
                .iter()
                .map(|(t, _)| t.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            // Every half hour from -12:00 to +14:00.
            let offset = rng.below(53) as i32 * 1800 - 12 * 3600;
            let tz = TimeZone::fixed(jiff::tz::Offset::from_seconds(offset).unwrap());
            let after =
                Timestamp::from_second(1_767_225_600 + rng.below(5 * 365 * 86_400) as i64).unwrap();
            let has = |f: usize, v: i64| fields[f].1[v as usize];
            let either = !fields[DAY].0.starts_with('*') && !fields[WEEKDAY].0.starts_with('*');
            let mut expected = None;
            let mut date = tz.to_datetime(after).date();
            'days: for _ in 0..MAX_DAYS {
                let (day, weekday) = (
                    has(DAY, date.day().into()),
                    has(WEEKDAY, date.weekday().to_sunday_zero_offset().into()),
                );
                if has(MONTH, date.month().into())
                    && if either {
                        day || weekday
                    } else {
                        day && weekday
                    }
                {
                    for h in 0..24 {
                        for m in 0..60 {
                            if has(HOUR, h) && has(MINUTE, m) {
                                let t = tz.to_timestamp(date.at(h as i8, m as i8, 0, 0)).unwrap();
                                if t > after {
                                    expected = Some(t);
                                    break 'days;
                                }
                            }
                        }
                    }
                }
                date = date.tomorrow().unwrap();
            }
            let cron = Cron::parse(&expr).unwrap_or_else(|e| panic!("case {case}: {expr}: {e:#}"));
            assert_eq!(
                cron.next_after(after, &tz),
                expected,
                "case {case}: {expr} after {after} at {offset}s"
            );
        }
    }

    #[test]
    fn parsing_never_panics() {
        let mut rng = Rng(0x0dd_ba11);
        let alphabet: Vec<char> = "0123456789*/-, \tjanfebmonsunJAN?L#".chars().collect();
        for _ in 0..5000 {
            let len = rng.below(24) as usize;
            let expr: String = (0..len)
                .map(|_| alphabet[rng.below(alphabet.len() as u64) as usize])
                .collect();
            if let Ok(cron) = Cron::parse(&expr) {
                let _ = cron.next_after(Timestamp::UNIX_EPOCH, &TimeZone::UTC);
            }
        }
        for expr in [
            "99999999999 * * * *",
            "*/256 * * * *",
            "1-99999 * * * *",
            "* * * * 8",
        ] {
            assert!(Cron::parse(expr).is_err(), "{expr}");
        }
    }

    #[test]
    fn day_fields_follow_vixie_cron() {
        let utc = TimeZone::UTC;
        // Both restricted: the 13th or any Friday.
        assert_eq!(
            next("0 0 13 * 5", "2026-11-01T00:00:00Z", &utc),
            "2026-11-06T00:00:00"
        );
        // One written with a star: both must match, so an odd-numbered Monday.
        assert_eq!(
            next("0 0 */2 * 1", "2026-11-01T00:00:00Z", &utc),
            "2026-11-09T00:00:00"
        );
    }

    #[test]
    fn follows_daylight_saving_time() {
        let tz = paris();
        // 29 March 2026: 02:00 jumps to 03:00. A run at 02:30 happens at 03:30, once.
        let c = Cron::parse("30 2 * * *").unwrap();
        let runs = c.upcoming(at("2026-03-28T00:00:00Z"), &tz, 3);
        let shown: Vec<String> = runs
            .iter()
            .map(|t| tz.to_datetime(*t).to_string())
            .collect();
        assert_eq!(
            shown,
            [
                "2026-03-28T02:30:00",
                "2026-03-29T03:30:00",
                "2026-03-30T02:30:00"
            ]
        );
        assert_eq!(runs[1], at("2026-03-29T01:30:00Z"));

        // 25 October 2026: 03:00 goes back to 02:00. A run at 02:30 happens the first time only.
        let runs = c.upcoming(at("2026-10-24T12:00:00Z"), &tz, 2);
        assert_eq!(runs[0], at("2026-10-25T00:30:00Z"), "02:30 summer time");
        assert_eq!(runs[1], at("2026-10-26T01:30:00Z"));

        // An hourly task skips no hour in spring and runs once per wall-clock hour in autumn.
        let hourly = Cron::parse("0 * * * *").unwrap();
        let spring = hourly.upcoming(at("2026-03-29T00:30:00Z"), &tz, 2);
        assert_eq!(
            spring,
            [at("2026-03-29T01:00:00Z"), at("2026-03-29T02:00:00Z")]
        );
        let autumn = hourly.upcoming(at("2026-10-24T23:30:00Z"), &tz, 3);
        assert_eq!(
            autumn,
            [
                at("2026-10-25T00:00:00Z"),
                at("2026-10-25T02:00:00Z"),
                at("2026-10-25T03:00:00Z"),
            ]
        );
        assert_eq!(
            hourly.last_until(at("2026-10-25T01:59:00Z"), &tz),
            Some(at("2026-10-25T00:00:00Z")),
            "02:00 is not repeated"
        );
    }
}
