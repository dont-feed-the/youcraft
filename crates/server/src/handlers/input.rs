//! What handlers accept from callers: the cursors of paged lists, and text people write.
//!
//! Every text a person gives the site, through a command or from Google, passes the same rule:
//! trimmed at both ends, no control character but the line breaks a field allows, none of the
//! characters that reorder the text around them (bidirectional embeddings, overrides and
//! isolates), and at most a limit of characters, counted as Unicode scalar values (what JSON
//! Schema's `maxLength` and the pages count).

use youcraft_api::{ApiError, Rule};
use youcraft_store::Window;

/// The farthest a cursor may lead into a list: past it, a list is searched, not paged.
const DEEPEST: u32 = 10_000;

/// The window of `limit` rows that `after` (the `next` of the previous page, or `None` for the
/// first) leads to.
pub fn window(after: Option<&str>, limit: u32) -> Result<Window, ApiError> {
    let offset = match after {
        None => 0,
        Some(after) => after
            .parse::<u32>()
            .ok()
            .filter(|offset| *offset <= DEEPEST)
            .ok_or_else(|| ApiError::invalid("after", Rule::Malformed))?,
    };
    Ok(Window { offset, limit })
}

/// The `next` of a page read through `window`: where the following one starts, if rows follow.
pub fn next(window: Window, more: bool) -> Option<String> {
    more.then(|| window.offset.saturating_add(window.limit).to_string())
}

/// `value` trimmed, if it is one line of 1 to `limit` characters that text may hold: a title,
/// a name, a query.
pub fn line(field: &str, value: &str, limit: usize) -> Result<String, ApiError> {
    accepted(value, limit, false).map_err(|rule| ApiError::invalid(field, rule))
}

/// `value` trimmed, with its line breaks written `\n`, if it holds 1 to `limit` characters that
/// text may hold: a comment.
pub fn text(field: &str, value: &str, limit: usize) -> Result<String, ApiError> {
    accepted(value, limit, true).map_err(|rule| ApiError::invalid(field, rule))
}

/// `value` as [`text`] takes it, or empty: for a description, which may be left blank.
pub fn optional_text(field: &str, value: &str, limit: usize) -> Result<String, ApiError> {
    if value.trim().is_empty() {
        Ok(String::new())
    } else {
        text(field, value, limit)
    }
}

/// `value` made into one line that text may hold, of at most `limit` characters: a name the
/// site did not write and cannot refuse (the one Google gives), its line breaks and tabs made
/// spaces and every other character text may not hold left out.
pub fn cleaned(value: &str, limit: usize) -> String {
    let kept: String = value
        .chars()
        .map(|c| {
            if allowed(c, true) && !allowed(c, false) {
                ' '
            } else {
                c
            }
        })
        .filter(|&c| allowed(c, false))
        .collect();
    let cut: String = kept.trim().chars().take(limit).collect();
    cut.trim_end().to_owned()
}

/// What [`line`] and [`text`] make of `value`, or the rule it breaks.
fn accepted(value: &str, limit: usize, lines: bool) -> Result<String, Rule> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Rule::Required);
    }
    if let Some(refused) = value.chars().find(|&c| !allowed(c, lines)) {
        return Err(if breaks_line(refused) {
            Rule::OneLine
        } else {
            Rule::Characters
        });
    }
    let value = if lines {
        value
            .replace("\r\n", "\n")
            .replace(['\r', '\u{2028}', '\u{2029}'], "\n")
    } else {
        value.to_owned()
    };
    if value.chars().count() > limit {
        return Err(Rule::TooLong {
            limit: u64::try_from(limit).unwrap_or(u64::MAX),
        });
    }
    Ok(value)
}

/// Whether text may hold `c`: no control character but the line breaks and tabs of text on
/// several `lines`, and no character that reorders the text around it.
fn allowed(c: char, lines: bool) -> bool {
    if breaks_line(c) || c == '\t' {
        return lines;
    }
    !c.is_control() && !reorders(c)
}

/// Whether `c` ends a line: line feed, carriage return, or Unicode's line and paragraph
/// separators.
fn breaks_line(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// Whether `c` is a bidirectional embedding, override or isolate (or one's end), which turn
/// the text after it around: what makes a name or a title read otherwise than it is written.
/// The marks that only lean a neutral character one way (U+200E, U+200F, U+061C) reorder
/// nothing around them and stay, as the joiners that Persian and emoji need do.
fn reorders(c: char) -> bool {
    matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursors_lead_where_the_last_page_ended() {
        let first = window(None, 20).unwrap();
        assert_eq!(next(first, true).as_deref(), Some("20"));
        assert_eq!(next(first, false), None);
        assert_eq!(window(Some("20"), 20).unwrap().offset, 20);
        for wrong in ["-1", "x", "99999999", ""] {
            assert_eq!(
                window(Some(wrong), 20),
                Err(ApiError::invalid("after", Rule::Malformed)),
                "{wrong}"
            );
        }
    }

    #[test]
    fn text_is_trimmed_bounded_and_free_of_controls() {
        let broken = |value: &str, lines: bool| accepted(value, 5, lines).err();
        assert_eq!(line("t", "  Hi  ", 5).unwrap(), "Hi");
        assert_eq!(text("t", "a\r\nb\rc", 5).unwrap(), "a\nb\nc");
        assert_eq!(text("t", "a\u{2029}b", 5).unwrap(), "a\nb");
        assert_eq!(broken("a\nb", false), Some(Rule::OneLine));
        assert_eq!(broken("a\u{2028}b", false), Some(Rule::OneLine));
        assert_eq!(broken("a\tb", false), Some(Rule::Characters));
        assert_eq!(broken("   ", true), Some(Rule::Required));
        assert_eq!(broken("a\u{7}", true), Some(Rule::Characters));
        assert!(line("t", "ééééé", 5).is_ok(), "characters, not bytes");
        assert!(
            line("t", "😀😀😀😀😀", 5).is_ok(),
            "scalar values, not UTF-16"
        );
        assert_eq!(broken("éééééé", false), Some(Rule::TooLong { limit: 5 }));
        assert_eq!(
            line("t", "abcdef", 5),
            Err(ApiError::invalid("t", Rule::TooLong { limit: 5 }))
        );
        assert_eq!(optional_text("d", " ", 5).unwrap(), "");
        assert_eq!(optional_text("d", " a ", 5).unwrap(), "a");
    }

    #[test]
    fn nothing_turns_text_around() {
        for reordering in [
            '\u{202A}', '\u{202B}', '\u{202D}', '\u{202E}', '\u{2066}', '\u{2069}',
        ] {
            let title = format!("abc{reordering}gpj.exe");
            assert_eq!(
                accepted(&title, 99, true),
                Err(Rule::Characters),
                "{reordering:?}"
            );
        }
        for kept in ["שלום\u{200F}!", "می\u{200C}خواهم", "👩\u{200D}💻"] {
            assert_eq!(line("t", kept, 99).unwrap(), kept);
        }
    }

    #[test]
    fn names_from_elsewhere_are_cleaned_not_refused() {
        assert_eq!(cleaned(" Ada\u{0}\nLovelace ", 100), "Ada Lovelace");
        assert_eq!(cleaned("\u{202E}evil", 100), "evil");
        assert_eq!(cleaned(&"é".repeat(300), 100).chars().count(), 100);
        assert_eq!(cleaned("ab cd", 3), "ab");
        assert_eq!(cleaned("\u{202E}\n", 100), "");
    }
}
