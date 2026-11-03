//! What a page asks of the browser it runs in: the document's theme, its cookies, a reload,
//! another address, the clipboard. On the server there is no browser, and each does nothing:
//! these only run from events, which happen in the browser.

#[cfg(all(feature = "hydrate", not(feature = "ssr")))]
mod inside {
    use wasm_bindgen::JsCast;
    use youcraft_api::commands::account::Theme;

    fn window() -> Option<web_sys::Window> {
        web_sys::window()
    }

    pub fn draw(theme: Theme) {
        let root = window()
            .and_then(|window| window.document())
            .and_then(|document| document.document_element());
        if let Some(root) = root {
            let _ = match theme {
                Theme::Device => root.remove_attribute("data-theme"),
                theme => root.set_attribute("data-theme", theme.as_str()),
            };
        }
    }

    pub fn remember(name: &str, value: &str) {
        let document = window()
            .and_then(|window| window.document())
            .and_then(|document| document.dyn_into::<web_sys::HtmlDocument>().ok());
        if let Some(document) = document {
            let max_age = youcraft_api::cookies::MAX_AGE;
            let _ = document.set_cookie(&format!(
                "{name}={value}; Path=/; Max-Age={max_age}; SameSite=Lax"
            ));
        }
    }

    pub fn reload() {
        if let Some(window) = window() {
            let _ = window.location().reload();
        }
    }

    pub fn go(address: &str) {
        if let Some(window) = window() {
            let _ = window.location().set_href(address);
        }
    }

    pub fn origin() -> String {
        window()
            .and_then(|window| window.location().origin().ok())
            .unwrap_or_default()
    }

    pub fn here() -> String {
        window().map_or_else(String::new, |window| {
            let location = window.location();
            let path = location.pathname().unwrap_or_default();
            let search = location.search().unwrap_or_default();
            format!("{path}{search}")
        })
    }

    pub fn copy(text: &str) {
        if let Some(window) = window() {
            let _ = window.navigator().clipboard().write_text(text);
        }
    }
}

#[cfg(feature = "ssr")]
mod inside {
    use youcraft_api::commands::account::Theme;

    pub fn draw(_: Theme) {}

    pub fn remember(_: &str, _: &str) {}

    pub fn reload() {}

    pub fn go(_: &str) {}

    pub fn origin() -> String {
        String::new()
    }

    pub fn here() -> String {
        String::new()
    }

    pub fn copy(_: &str) {}
}

/// Put `text` on the clipboard.
pub use inside::copy;
/// Draw the document in `theme`.
pub use inside::draw;
/// Go to `address`.
pub use inside::go;
/// The path and query of the page shown.
pub use inside::here;
/// The site's origin, `https://youcraft.tube`.
pub use inside::origin;
/// Load the page again from the server.
pub use inside::reload;
/// Keep `value` in the cookie `name`, which the server reads to render the next pages.
pub use inside::remember;
