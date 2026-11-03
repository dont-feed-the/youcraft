//! Who is signed in, for every component, and the changes a person makes to how the site looks
//! to them: theme and language, kept in their account when signed in and in their browser's
//! cookies either way, so a signed-out visitor keeps them too.

use leptos::prelude::*;
use youcraft_api::commands::account::{self, Me, Preferences, Theme};
use youcraft_api::commands::site::Nothing;

use crate::call::call;

/// The signed-in person, `None` when nobody is (or the site cannot tell).
#[derive(Clone, Copy)]
pub struct SignedIn(pub Resource<Option<Me>>);

impl SignedIn {
    /// Ask the site who is signed in, for the components below.
    pub fn provide() {
        let me = Resource::new(
            || (),
            |()| async { call::<account::Get>(Nothing {}).await.ok().flatten() },
        );
        provide_context(SignedIn(me));
    }

    /// The signed-in person's resource.
    pub fn resource() -> Option<Resource<Option<Me>>> {
        use_context::<SignedIn>().map(|signed_in| signed_in.0)
    }
}

/// The theme the page shows now, which the person may change.
#[derive(Clone, Copy)]
pub struct Shown(pub RwSignal<Theme>);

/// Draw the page in `theme` from now on, for `me` if signed in.
pub fn choose_theme(theme: Theme, me: Option<&Me>) {
    if let Some(Shown(shown)) = use_context::<Shown>() {
        shown.set(theme);
    }
    browser::draw(theme);
    browser::remember(youcraft_api::cookies::THEME, theme.as_str());
    if let Some(me) = me {
        let preferences = Preferences {
            language: me.preferences.language.clone(),
            theme,
        };
        leptos::task::spawn_local(async move {
            let _ = call::<account::SetPreferences>(preferences).await;
        });
    }
}

/// Show the site in the language `code` from now on, for `me` if signed in: the page is drawn
/// again by the server, in that language.
pub fn choose_language(code: String, me: Option<&Me>) {
    browser::remember(youcraft_api::cookies::LANGUAGE, &code);
    let theme = use_context::<Shown>().map_or(
        me.map_or(Theme::Device, |me| me.preferences.theme),
        |Shown(shown)| shown.get_untracked(),
    );
    let preferences = me.map(|_| Preferences {
        language: Some(code),
        theme,
    });
    leptos::task::spawn_local(async move {
        if let Some(preferences) = preferences {
            let _ = call::<account::SetPreferences>(preferences).await;
        }
        browser::reload();
    });
}

/// Sign this browser out, then draw the page again, signed out.
pub fn sign_out() {
    leptos::task::spawn_local(async move {
        let _ = call::<account::SignOut>(Nothing {}).await;
        browser::reload();
    });
}

#[cfg(all(feature = "hydrate", not(feature = "ssr")))]
mod browser {
    //! The browser's side of a change: the document's theme, its cookies, a reload.

    use wasm_bindgen::JsCast;
    use youcraft_api::commands::account::Theme;

    pub fn draw(theme: Theme) {
        let root = web_sys::window()
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
        let document = web_sys::window()
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
        if let Some(window) = web_sys::window() {
            let _ = window.location().reload();
        }
    }
}

#[cfg(feature = "ssr")]
mod browser {
    //! On the server there is no browser to change: events only happen in the browser.

    use youcraft_api::commands::account::Theme;

    pub fn draw(_: Theme) {}

    pub fn remember(_: &str, _: &str) {}

    pub fn reload() {}
}
