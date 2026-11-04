//! What a page asks of the browser it runs in: the document's theme, its cookies, a reload,
//! another address, the clipboard, the text of a field, a timer. On the server there is no
//! browser, and each does nothing: these only run from events and effects, which happen in the
//! browser.

use leptos::prelude::*;

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

    pub fn show(event: &leptos::ev::Event, text: &str) {
        if let Some(field) = event.target() {
            let _ = js_sys::Reflect::set(&field, &"value".into(), &text.into());
        }
    }

    pub fn wait(
        time: std::time::Duration,
        then: impl FnOnce() + 'static,
    ) -> Option<leptos::prelude::TimeoutHandle> {
        leptos::prelude::set_timeout_with_handle(then, time).ok()
    }

    pub fn cancel(handle: leptos::prelude::TimeoutHandle) {
        handle.clear();
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

    pub fn show(_: &leptos::ev::Event, _: &str) {}

    pub fn wait(
        _: std::time::Duration,
        _: impl FnOnce() + 'static,
    ) -> Option<leptos::prelude::TimeoutHandle> {
        None
    }

    pub fn cancel(_: leptos::prelude::TimeoutHandle) {}
}

/// The text of the field `event` came from, cut to `limit` characters, which the field then
/// shows. Characters are counted as the server counts every limit, as Unicode scalar values;
/// the browser's own `maxlength` counts UTF-16 units, and would stop a title of emoji halfway.
pub fn clipped(event: &leptos::ev::Event, limit: usize) -> String {
    let text = event_target_value(event);
    match text.char_indices().nth(limit) {
        Some((end, _)) => {
            let cut = text.get(..end).unwrap_or_default().to_owned();
            inside::show(event, &cut);
            cut
        }
        None => text,
    }
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

/// One timer of a part of the page: each wait it starts replaces the one it started before, and
/// it stops when that part of the page goes, so a page never runs two of its loops at once, nor
/// one after it is gone.
#[derive(Clone, Copy)]
pub struct Timer(StoredValue<Option<TimeoutHandle>>);

impl Default for Timer {
    fn default() -> Timer {
        Timer::new()
    }
}

impl Timer {
    /// A timer that waits for nothing yet, for the part of the page being built.
    pub fn new() -> Timer {
        let timer = Timer(StoredValue::new(None));
        on_cleanup(move || timer.stop());
        timer
    }

    /// Run `then` once `time` has passed, instead of what the timer was waiting to run.
    pub fn after(self, time: std::time::Duration, then: impl FnOnce() + 'static) {
        self.stop();
        if self.0.try_get_value().is_none() {
            // Its part of the page is gone.
            return;
        }
        let handle = inside::wait(time, then);
        let _ = self.0.try_update_value(|waiting| *waiting = handle);
    }

    /// Run nothing of what the timer was waiting to run.
    pub fn stop(self) {
        if let Some(Some(handle)) = self.0.try_update_value(Option::take) {
            inside::cancel(handle);
        }
    }
}
