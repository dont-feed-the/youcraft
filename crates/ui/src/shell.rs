//! The HTML document around every page: language, direction and theme, the stylesheet, what the
//! browser hydrates with (catalog, render time, theme, languages), the hydration scripts under a
//! per-request nonce, and the content security policy that admits nothing from anywhere else.

use leptos::prelude::*;
use leptos_meta::MetaTags;
use youcraft_api::commands::account::Theme;

use crate::app::App;
use crate::lang::{CARRIER, Language, RenderTime, carried, theme};

/// The document for one request; the server provides the [`Language`] and a nonce first.
pub fn shell(options: LeptosOptions) -> impl IntoView {
    let language = use_context::<Language>().unwrap_or(Language(youcraft_i18n::Lang::EN));
    let time = use_context::<RenderTime>().unwrap_or(RenderTime(0));
    let theme = theme();
    let drawn = (theme != Theme::Device).then_some(theme.as_str());
    let code = youcraft_i18n::code(language.0);
    let dir = if youcraft_i18n::rtl(language.0) {
        "rtl"
    } else {
        "ltr"
    };
    #[cfg(feature = "ssr")]
    send_security_policy();
    view! {
        <!DOCTYPE html>
        <html lang=code dir=dir data-theme=drawn>
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <meta name="color-scheme" content="light dark"/>
                <link rel="icon" href="/favicon.svg" type="image/svg+xml"/>
                <link rel="stylesheet" href="/pkg/youcraft.css"/>
                <script type="application/json" id=CARRIER inner_html=carried(language, time, theme)></script>
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

/// Send the content security policy of the page being rendered.
#[cfg(feature = "ssr")]
fn send_security_policy() {
    use http::header::{CONTENT_SECURITY_POLICY, HeaderValue};
    use leptos::nonce::use_nonce;

    let nonce = use_nonce().map(|nonce| nonce.to_string());
    let path = use_context::<http::request::Parts>().map(|parts| parts.uri.path().to_owned());
    let policy = security_policy(path.as_deref().unwrap_or_default(), nonce.as_deref());
    if let (Some(response), Ok(value)) = (
        use_context::<leptos_axum::ResponseOptions>(),
        HeaderValue::from_str(&policy),
    ) {
        response.insert_header(CONTENT_SECURITY_POLICY, value);
    }
}

/// The content security policy of the page at `path`: our own origin only, scripts by `nonce`,
/// and WebAssembly compiled from our own files; only an embedded player may be framed elsewhere.
pub fn security_policy(path: &str, nonce: Option<&str>) -> String {
    let nonce = nonce
        .map(|nonce| format!(" 'nonce-{nonce}'"))
        .unwrap_or_default();
    let framers = if youcraft_api::routes::is_embed(path) {
        "*"
    } else {
        "'self'"
    };
    format!(
        "default-src 'self'; script-src 'self'{nonce} 'wasm-unsafe-eval'; style-src 'self'; \
         img-src 'self' data: blob:; media-src 'self' blob:; connect-src 'self'; font-src 'self'; \
         object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors {framers}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use youcraft_api::{VideoId, routes};

    #[test]
    fn only_the_embedded_player_may_be_framed_by_other_sites() {
        let video = VideoId::from_bits(42);
        assert!(security_policy(&routes::embed(video), None).ends_with("frame-ancestors *"));
        for path in [
            routes::watch(video),
            routes::HOME.to_owned(),
            "/embedded".to_owned(),
        ] {
            let policy = security_policy(&path, Some("n"));
            assert!(
                policy.ends_with("frame-ancestors 'self'"),
                "{path}: {policy}"
            );
            assert!(policy.contains("script-src 'self' 'nonce-n' "), "{policy}");
        }
    }
}
