//! The language the player writes its labels in, given by the page that holds it.

use leptos::prelude::*;
use youcraft_i18n::Lang;

/// The language of the page around the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerLanguage(pub Lang);

/// The language the player is rendered in; English outside a player.
pub fn current() -> Lang {
    use_context::<PlayerLanguage>().map_or(Lang::EN, |language| language.0)
}

/// `source` in the player's language.
pub fn tr(source: &'static str) -> &'static str {
    youcraft_i18n::t(current(), source)
}

/// A count in the player's language: `one` and `other` are the English forms with `{n}`.
pub fn count(n: u64, one: &str, other: &str) -> String {
    youcraft_i18n::count(current(), n, one, other)
}
