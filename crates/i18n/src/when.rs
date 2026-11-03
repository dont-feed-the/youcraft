//! Times as a page writes them: how long ago, and how long a video lasts.

use craft_i18n::Lang;

use crate::count;

const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;
const WEEK: u64 = 7 * DAY;
/// A month and a year as averages of the Gregorian calendar: 400 years hold 146,097 days.
const YEAR: u64 = 146_097 * DAY / 400;
const MONTH: u64 = YEAR / 12;

/// How long ago `then` was at `now` (seconds since 1970), in the largest whole unit: "3 days
/// ago". A time in the future reads as zero seconds ago.
pub fn ago(lang: Lang, then: i64, now: i64) -> String {
    let elapsed = u64::try_from(now.saturating_sub(then)).unwrap_or(0);
    let units: [(u64, &str, &str); 7] = [
        (YEAR, "{n} year ago", "{n} years ago"),
        (MONTH, "{n} month ago", "{n} months ago"),
        (WEEK, "{n} week ago", "{n} weeks ago"),
        (DAY, "{n} day ago", "{n} days ago"),
        (HOUR, "{n} hour ago", "{n} hours ago"),
        (MINUTE, "{n} minute ago", "{n} minutes ago"),
        (1, "{n} second ago", "{n} seconds ago"),
    ];
    let (size, one, other) = units
        .into_iter()
        .find(|(size, _, _)| elapsed >= *size)
        .unwrap_or((1, "{n} second ago", "{n} seconds ago"));
    count(lang, elapsed / size, one, other)
}

/// A video's length as its badge shows it: `0:07`, `12:34`, `1:02:03`.
pub fn clock(seconds: u64) -> String {
    let (hours, minutes, seconds) = (seconds / HOUR, seconds % HOUR / MINUTE, seconds % MINUTE);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clocks_drop_empty_hours_only() {
        assert_eq!(clock(7), "0:07");
        assert_eq!(clock(754), "12:34");
        assert_eq!(clock(3723), "1:02:03");
    }

    #[test]
    fn ago_takes_the_largest_whole_unit() {
        let english = Lang::EN;
        assert_eq!(ago(english, 0, 59), "59 seconds ago");
        assert_eq!(ago(english, 0, 3600), "1 hour ago");
        assert_eq!(ago(english, 0, 13 * 86_400), "1 week ago");
        assert_eq!(ago(english, 0, 400 * 86_400), "1 year ago");
        assert_eq!(ago(english, 10, 0), "0 seconds ago");
    }
}
