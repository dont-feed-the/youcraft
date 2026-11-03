//! The reader's language and the time of rendering, given to every component.
//!
//! The server picks the language for each request and provides a [`Language`] and a
//! [`RenderTime`]; the page carries both, with the language's catalog, to the browser, which
//! installs them before it hydrates, so both sides write the same words and the same "3 days
//! ago".

use leptos::prelude::*;
use youcraft_i18n::Lang;

/// The language a page is written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Language(pub Lang);

/// The time the page was rendered, in seconds since 1970, from which ages are counted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderTime(pub i64);

impl RenderTime {
    /// The time of the page being rendered; 0 outside a page.
    pub fn current() -> i64 {
        use_context::<RenderTime>().map_or(0, |time| time.0)
    }
}

/// The id of the element that carries the language and render time to the browser.
pub const CARRIER: &str = "youcraft-page";

impl Language {
    /// The language a component is rendered in; English outside a page.
    pub fn current() -> Lang {
        use_context::<Language>().map_or(Lang::EN, |language| language.0)
    }
}

/// What the page carries to the browser: `{"code", "catalog", "now"}`, safe inside `<script>`.
pub fn carried(language: Language, time: RenderTime) -> String {
    let value = serde_json::json!({
        "code": youcraft_i18n::code(language.0),
        "catalog": youcraft_i18n::catalog_text(language.0),
        "now": time.0,
    });
    value.to_string().replace('<', "\\u003c")
}

/// The language and render time the page carries, the catalog installed; English and 0 if it
/// carries none.
#[cfg(feature = "hydrate")]
pub fn from_page() -> (Language, RenderTime) {
    let text = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(CARRIER))
        .and_then(|element| element.text_content())
        .unwrap_or_default();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    let code = value["code"].as_str().unwrap_or("en");
    youcraft_i18n::install(code, value["catalog"].as_str().unwrap_or(""));
    let language = Language(youcraft_i18n::i18n().from_code(code).unwrap_or(Lang::EN));
    (language, RenderTime(value["now"].as_i64().unwrap_or(0)))
}

/// `source`, in the language of the page being rendered.
pub fn tr(source: &'static str) -> &'static str {
    youcraft_i18n::t(Language::current(), source)
}

/// A count in the language of the page: `one` and `other` are the English forms with `{n}`.
pub fn count(n: u64, one: &str, other: &str) -> String {
    youcraft_i18n::count(Language::current(), n, one, other)
}
