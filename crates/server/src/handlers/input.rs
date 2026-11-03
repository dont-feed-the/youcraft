//! What handlers accept from callers: the cursors of paged lists, and text people write.

use youcraft_api::ApiError;
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
            .ok_or_else(|| ApiError::invalid("after", "the next of a previous page"))?,
    };
    Ok(Window { offset, limit })
}

/// The `next` of a page read through `window`: where the following one starts, if rows follow.
pub fn next(window: Window, more: bool) -> Option<String> {
    more.then(|| window.offset.saturating_add(window.limit).to_string())
}

/// `value` trimmed, if it holds 1 to `limit` characters and no control character but line
/// breaks (and those only where `lines` allows them).
pub fn text(field: &str, value: &str, limit: usize, lines: bool) -> Result<String, ApiError> {
    let value = value.trim();
    let length = value.chars().count();
    let clean = value
        .chars()
        .all(|c| !c.is_control() || (lines && matches!(c, '\n' | '\r' | '\t')));
    if length == 0 || length > limit || !clean {
        let rule = if lines {
            format!("1 to {limit} characters")
        } else {
            format!("1 to {limit} characters on one line")
        };
        return Err(ApiError::invalid(field, &rule));
    }
    Ok(value.replace("\r\n", "\n"))
}

/// `value` as [`text`] takes it, or empty: for a description, which may be left blank.
pub fn optional_text(field: &str, value: &str, limit: usize) -> Result<String, ApiError> {
    if value.trim().is_empty() {
        Ok(String::new())
    } else {
        text(field, value, limit, true)
    }
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
            assert!(window(Some(wrong), 20).is_err(), "{wrong}");
        }
    }

    #[test]
    fn text_is_trimmed_bounded_and_free_of_controls() {
        assert_eq!(text("t", "  Hi  ", 5, false).unwrap(), "Hi");
        assert_eq!(text("t", "a\r\nb", 5, true).unwrap(), "a\nb");
        assert!(text("t", "a\nb", 5, false).is_err());
        assert!(text("t", "   ", 5, true).is_err());
        assert!(
            text("t", "ééééé", 5, false).is_ok(),
            "characters, not bytes"
        );
        assert!(text("t", "éééééé", 5, false).is_err());
        assert!(text("t", "a\u{7}", 5, true).is_err());
        assert_eq!(optional_text("d", " ", 5).unwrap(), "");
    }
}
