//! The reader's language, theme and the time of rendering, given to every component.
//!
//! The server picks the language and theme for each request and provides a [`Language`], a
//! [`Theme`] and a [`RenderTime`]; the page carries them, with the language's catalog and the
//! languages on offer, to the browser, which installs them before it hydrates, so both sides
//! write the same words and the same "3 days ago".

use leptos::prelude::*;
use youcraft_api::commands::account::Theme;
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

/// What the page carries to the browser: `{"code", "catalog", "now", "theme", "languages"}`,
/// safe inside `<script>`.
pub fn carried(language: Language, time: RenderTime, theme: Theme) -> String {
    let value = serde_json::json!({
        "code": youcraft_i18n::code(language.0),
        "catalog": youcraft_i18n::catalog_text(language.0),
        "now": time.0,
        "theme": theme.as_str(),
        "languages": offered(),
    });
    value.to_string().replace('<', "\\u003c")
}

/// The languages a person may choose, as `(code, its own name)`, English first.
#[cfg(feature = "ssr")]
pub fn offered() -> Vec<(String, String)> {
    youcraft_i18n::languages()
        .into_iter()
        .map(|(lang, name)| (youcraft_i18n::code(lang).to_owned(), name.to_owned()))
        .collect()
}

/// The languages a person may choose, as `(code, its own name)`, as the page carries them.
#[cfg(all(feature = "hydrate", not(feature = "ssr")))]
pub fn offered() -> Vec<(String, String)> {
    OFFERED.get().cloned().unwrap_or_default()
}

#[cfg(all(feature = "hydrate", not(feature = "ssr")))]
static OFFERED: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();

/// The theme the page is drawn in; the device's outside a page.
pub fn theme() -> Theme {
    use_context::<Theme>().unwrap_or_default()
}

/// The language, render time and theme the page carries, the catalog installed and the
/// languages on offer kept; English, 0 and the device's theme if it carries none.
#[cfg(feature = "hydrate")]
pub fn from_page() -> (Language, RenderTime, Theme) {
    let text = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(CARRIER))
        .and_then(|element| element.text_content())
        .unwrap_or_default();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    let code = value
        .get("code")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("en");
    youcraft_i18n::install(
        code,
        value
            .get("catalog")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(""),
    );
    let language = Language(youcraft_i18n::i18n().from_code(code).unwrap_or(Lang::EN));
    let theme = value
        .get("theme")
        .and_then(serde_json::Value::as_str)
        .and_then(Theme::from_name)
        .unwrap_or_default();
    #[cfg(not(feature = "ssr"))]
    {
        let languages: Vec<(String, String)> =
            serde_json::from_value(value.get("languages").cloned().unwrap_or_default())
                .unwrap_or_default();
        let _ = OFFERED.set(languages);
    }
    (
        language,
        RenderTime(
            value
                .get("now")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0),
        ),
        theme,
    )
}

/// `source`, in the language of the page being rendered.
pub fn tr(source: &'static str) -> &'static str {
    youcraft_i18n::t(Language::current(), source)
}

/// `template`, already translated with [`tr`], with each `{name}` filled with its value.
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    youcraft_i18n::fill(template, values)
}

/// A count in the language of the page, the number in full (`1,234 comments`): `one` and
/// `other` are the English forms with `{n}`.
pub fn count(n: u64, one: &str, other: &str) -> String {
    youcraft_i18n::count(Language::current(), n, one, other)
}

/// A count in the language of the page, the number compact as views and subscribers are shown:
/// `1.2K views`.
pub fn compact_count(n: u64, one: &str, other: &str) -> String {
    youcraft_i18n::compact_count(Language::current(), n, one, other)
}

/// A bare number in the language of the page, compact as a like button shows it: `1.2K`.
pub fn compact(n: u64) -> String {
    youcraft_i18n::compact(Language::current(), n)
}
