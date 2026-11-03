//! Every page and component of YouCraft.
//!
//! The server renders a page with the `ssr` feature and the browser hydrates it with `hydrate`
//! (built by `cargo xtask site`). Both run the same components: data comes from commands
//! ([`call`]), text from the catalogs in the reader's language ([`lang`]), and the layout follows
//! youtube.com on wide screens and m.youtube.com on phones, with YouCraft's own look.

#[cfg(not(any(feature = "ssr", feature = "hydrate")))]
compile_error!("youcraft-ui is built with its `ssr` feature (server) or `hydrate` (browser)");

pub mod app;
pub mod call;
pub mod components;
pub mod icons;
pub mod lang;
pub mod pages;
pub mod shell;

pub use app::App;
pub use shell::shell;

/// Start the page in the browser: install the catalog the page carries, then hydrate.
#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    let (language, time) = lang::from_page();
    leptos::mount::hydrate_body(move || {
        leptos::prelude::provide_context(language);
        leptos::prelude::provide_context(time);
        leptos::prelude::view! { <App/> }
    });
}
