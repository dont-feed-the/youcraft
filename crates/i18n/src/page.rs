//! The language of the page being rendered, as a Leptos context, and the lookups written in it.
//!
//! The server provides a [`Language`] at the root of each page it renders, and the browser the
//! one its page carries before it hydrates; every component below, the player's included, writes
//! its words in that language through [`tr`], [`count`], [`compact_count`] and [`compact`].

use leptos::prelude::use_context;

use crate::Lang;

/// The language a page is written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Language(pub Lang);

impl Language {
    /// The language a component is rendered in; English outside a page.
    pub fn current() -> Lang {
        use_context::<Language>().map_or(Lang::EN, |language| language.0)
    }
}

/// `source`, in the language of the page being rendered.
pub fn tr(source: &'static str) -> &'static str {
    crate::t(Language::current(), source)
}

/// A count in the language of the page, the number in full (`1,234 comments`): `one` and
/// `other` are the English forms with `{n}`.
pub fn count(n: u64, one: &str, other: &str) -> String {
    crate::count(Language::current(), n, one, other)
}

/// A count in the language of the page, the number compact as views and subscribers are shown:
/// `1.2K views`.
pub fn compact_count(n: u64, one: &str, other: &str) -> String {
    crate::compact_count(Language::current(), n, one, other)
}

/// A bare number in the language of the page, compact as a like button shows it: `1.2K`.
pub fn compact(n: u64) -> String {
    crate::compact(Language::current(), n)
}
