//! The reader's language, theme and the time of rendering, given to every component.
//!
//! The server picks the language and theme for each request and provides a [`Language`], a
//! [`Theme`] and a [`RenderTime`]; the page carries them, with the language's catalog and the
//! languages on offer, to the browser, which installs them before it hydrates, so both sides
//! write the same words and the same "3 days ago". The language and the lookups written in it
//! are i18n's (`youcraft_i18n::page`), which the player reads too.

use leptos::prelude::*;
use youcraft_api::commands::account::Theme;
#[cfg(feature = "hydrate")]
use youcraft_i18n::Lang;
pub use youcraft_i18n::fill;
pub use youcraft_i18n::page::{Language, compact, compact_count, count, tr};

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
