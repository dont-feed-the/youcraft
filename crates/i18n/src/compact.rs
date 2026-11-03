//! Numbers as each language writes them, in full (`123,456`, `1.234.567`, `12,34,567`) or
//! compact as counts of views and subscribers are shown (`1.2K`, `1,2 k`, `1.2万`, `1.2 ألف`).
//!
//! Full numbers are craft-i18n's `Format` of the language: its digits, separators and grouping.
//! Compact ones follow CLDR's short patterns (`compact_data.rs`) and never overstate: the shown
//! number is cut down, not rounded, to one decimal under 10 and to a whole number from 10 up, so
//! 1,999 views is `1.9K`. A number a pattern leaves in full (German thousands, `0`) is grouped
//! only from five digits, as CLDR's compact notation groups.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use craft_i18n::{Format, Lang};

use crate::compact_data::PATTERNS;

/// One power of ten of a language's short patterns: the integer digits of the number shown
/// (`00K` is 2; 0 writes the number in full), the text around it, and the texts a number shown
/// takes instead, by that number in tenths (Arabic `3 آلاف` against `1.5 ألف`).
pub(crate) struct Magnitude {
    digits: u8,
    before: &'static str,
    after: &'static str,
    instead: &'static [(u32, &'static str, &'static str)],
}

impl Magnitude {
    pub(crate) const fn new(
        digits: u8,
        before: &'static str,
        after: &'static str,
        instead: &'static [(u32, &'static str, &'static str)],
    ) -> Magnitude {
        Magnitude {
            digits,
            before,
            after,
            instead,
        }
    }
}

/// The number format of `lang`, made once per language.
pub(crate) fn format(lang: Lang) -> Arc<Format> {
    static FORMATS: Mutex<Option<HashMap<&'static str, Arc<Format>>>> = Mutex::new(None);
    let code = crate::code(lang);
    let mut formats = FORMATS.lock().unwrap_or_else(PoisonError::into_inner);
    let formats = formats.get_or_insert_with(HashMap::new);
    Arc::clone(
        formats
            .entry(code)
            .or_insert_with(|| Arc::new(Format::from_tag(code))),
    )
}

/// `n` in full, grouped as `lang` groups: `1,234,567`, `1 234 567`, `١٬٢٣٤٬٥٦٧`.
pub(crate) fn full(lang: Lang, n: u64) -> String {
    format(lang).integer(n)
}

/// `n` compact, as `lang` writes counts of views (`999`, `1.2K`, `12K`, `1.2M`), and the number
/// that shows (1200 for `1.2K`), whose plural form the words after it take.
pub(crate) fn compact(lang: Lang, n: u64) -> (String, u64) {
    let format = format(lang);
    let code = crate::code(lang);
    let magnitudes = PATTERNS
        .iter()
        .find(|(c, _)| *c == code)
        .or_else(|| PATTERNS.iter().find(|(c, _)| *c == "en"))
        .map_or(&[][..], |(_, magnitudes)| *magnitudes);
    let top = u32::try_from(magnitudes.len()).map_or(0, |len| len.saturating_add(2));
    let exponent = n.checked_ilog10().unwrap_or(0).min(top);
    let Some(magnitude) = exponent
        .checked_sub(3)
        .and_then(|index| magnitudes.get(usize::try_from(index).ok()?))
        .filter(|m| m.digits > 0)
    else {
        return (in_full(&format, n), n);
    };
    // The number shown is n / 10^shift, with `digits` integer digits; under 10 it keeps one
    // decimal, so it is counted in tenths, units of 10^(shift - 1).
    let shift = exponent.saturating_sub(u32::from(magnitude.digits).saturating_sub(1));
    let tenth = 10_u64.pow(shift.saturating_sub(1));
    let tenths = n / tenth;
    let tenths = if tenths < 100 {
        tenths
    } else {
        tenths - tenths % 10
    };
    let text = if tenths % 10 == 0 {
        in_full(&format, tenths / 10)
    } else {
        format.number(tenths as f64 / 10.0, 1)
    };
    let (before, after) = magnitude
        .instead
        .iter()
        .find(|(t, _, _)| u64::from(*t) == tenths)
        .map_or((magnitude.before, magnitude.after), |(_, b, a)| (b, a));
    (
        format!("{before}{text}{after}"),
        tenths.saturating_mul(tenth),
    )
}

/// `n` as compact notation writes a whole number, in a pattern or alone: grouped only from five
/// digits, so `1234`, `12.345`, `1000T`.
fn in_full(format: &Format, n: u64) -> String {
    if n < 10_000 {
        format.number_ungrouped(n as f64, 0)
    } else {
        format.integer(n)
    }
}

#[cfg(test)]
mod tests {
    use icu_decimal::CompactDecimalFormatter;
    use icu_decimal::input::Decimal;
    use icu_locale_core::Locale;

    use super::*;

    fn lang(code: &str) -> Lang {
        crate::i18n().from_code(code).unwrap()
    }

    /// Numbers across every magnitude: the representative ones, the edges of each power of ten,
    /// and a fixed pseudo-random sweep.
    fn numbers() -> Vec<u64> {
        let mut out = vec![
            0, 1, 999, 1000, 1234, 1999, 3000, 3500, 9999, 10_000, 12_345, 99_999,
        ];
        out.extend([
            123_456,
            999_999,
            1_234_567,
            12_345_678,
            123_456_789,
            1_000_000_000,
        ]);
        out.extend([
            10_000_000_000,
            12_000_000_000,
            1_000_000_000_000,
            10_u64.pow(14),
        ]);
        out.extend([10_u64.pow(15), 1_234_567_890_123_456, u64::MAX]);
        for power in 3..19 {
            out.extend([
                10_u64.pow(power) - 1,
                10_u64.pow(power),
                3 * 10_u64.pow(power),
            ]);
        }
        let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
        for _ in 0..3000 {
            x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            out.push((x >> 1) >> (x % 60));
        }
        out
    }

    /// ICU4X's compact formatting of `n` cut down, not rounded, to the digits it shows.
    fn oracle(icu: &CompactDecimalFormatter, n: u64) -> String {
        let magnitude = n.checked_ilog10().unwrap_or(0);
        let exponent = u32::from(icu.compact_exponent_for_magnitude(magnitude as i16));
        let kept = if magnitude > exponent {
            exponent
        } else {
            magnitude.saturating_sub(1)
        };
        let cut = n - n % 10_u64.pow(kept);
        icu.format_to_string(&Decimal::from(cut))
    }

    #[test]
    fn compact_numbers_are_icu4x_cut_down_in_every_language() {
        for (code, _) in PATTERNS {
            let tag = match *code {
                "pt-br" => "pt",
                other => other,
            };
            let locale = Locale::try_from_str(tag).unwrap();
            let icu = CompactDecimalFormatter::try_new_short((&locale).into(), Default::default())
                .unwrap();
            for n in numbers() {
                let (text, shown) = compact(lang(code), n);
                assert_eq!(text, oracle(&icu, n), "{code} {n}");
                assert!(shown <= n, "{code} {n}: {shown}");
            }
        }
    }

    #[test]
    fn compact_numbers_never_overstate() {
        for (n, text, shown) in [
            (999, "999", 999),
            (1000, "1K", 1000),
            (1999, "1.9K", 1900),
            (12_999, "12K", 12_000),
            (999_999, "999K", 999_000),
            (1_000_000, "1M", 1_000_000),
            (1_250_000_000, "1.2B", 1_200_000_000),
        ] {
            assert_eq!(compact(Lang::EN, n), (text.to_owned(), shown), "{n}");
        }
    }

    #[test]
    fn compact_numbers_as_each_language_writes_them() {
        for (code, n, shown) in [
            ("fr", 1234, "1,2\u{a0}k"),
            ("de", 1234, "1234"),
            ("de", 12_345, "12.345"),
            ("de", 1_234_567, "1,2\u{a0}Mio."),
            ("ja", 12_345, "1.2万"),
            ("ja", 123_456_789, "1.2億"),
            ("ar", 3000, "3\u{a0}آلاف"),
            ("ar", 3500, "3.5\u{a0}ألف"),
            ("fa", 1234, "۱٫۲\u{a0}هزار"),
            ("bn", 1_234_567, "১২\u{a0}লা"),
            ("ru", 1234, "1,2\u{a0}тыс."),
        ] {
            assert_eq!(compact(lang(code), n).0, shown, "{code} {n}");
        }
    }

    #[test]
    fn full_numbers_group_as_each_language_does() {
        for (code, shown) in [
            ("fr", "1\u{202f}234\u{202f}567"),
            ("de", "1.234.567"),
            ("bn", "১২,৩৪,৫৬৭"),
            ("fa", "۱٬۲۳۴٬۵۶۷"),
            ("ar", "1,234,567"),
        ] {
            assert_eq!(full(lang(code), 1_234_567), shown, "{code}");
        }
        assert_eq!(full(Lang::EN, 1_234_567), "1,234,567");
    }

    #[test]
    fn one_decimal_reads_exactly() {
        let en = format(Lang::EN);
        for tenths in 10..100_u64 {
            let want = format!("{}.{}", tenths / 10, tenths % 10);
            assert_eq!(en.number(tenths as f64 / 10.0, 1), want);
        }
    }
}
