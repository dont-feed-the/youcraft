//! The HTML document around every page: language and direction, the stylesheet, what the browser
//! hydrates with (catalog and render time), the hydration scripts under a per-request nonce, and the content
//! security policy that admits nothing from anywhere else.

use leptos::prelude::*;
use leptos_meta::MetaTags;

use crate::app::App;
use crate::lang::{CARRIER, Language, RenderTime, carried};

/// The document for one request; the server provides the [`Language`] and a nonce first.
pub fn shell(options: LeptosOptions) -> impl IntoView {
    let language = use_context::<Language>().unwrap_or(Language(youcraft_i18n::Lang::EN));
    let time = use_context::<RenderTime>().unwrap_or(RenderTime(0));
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
        <html lang=code dir=dir>
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <meta name="color-scheme" content="light dark"/>
                <link rel="icon" href="/favicon.svg" type="image/svg+xml"/>
                <link rel="stylesheet" href="/pkg/youcraft.css"/>
                <script type="application/json" id=CARRIER inner_html=carried(language, time)></script>
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

/// Send the content security policy: our own origin only, scripts by this request's nonce, and
/// WebAssembly compiled from our own files.
#[cfg(feature = "ssr")]
fn send_security_policy() {
    use http::header::{CONTENT_SECURITY_POLICY, HeaderValue};
    use leptos::nonce::use_nonce;

    let nonce = use_nonce()
        .map(|nonce| format!(" 'nonce-{nonce}'"))
        .unwrap_or_default();
    let policy = format!(
        "default-src 'self'; script-src 'self'{nonce} 'wasm-unsafe-eval'; style-src 'self'; \
         img-src 'self' data: blob:; media-src 'self' blob:; connect-src 'self'; font-src 'self'; \
         object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'self'"
    );
    if let (Some(response), Ok(value)) = (
        use_context::<leptos_axum::ResponseOptions>(),
        HeaderValue::from_str(&policy),
    ) {
        response.insert_header(CONTENT_SECURITY_POLICY, value);
    }
}
