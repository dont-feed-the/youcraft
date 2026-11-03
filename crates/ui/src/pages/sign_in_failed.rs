//! The page a sign-in that did not work ends on, `/signin/failed?why=…&next=…`: why, and the
//! way to try again.

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_query_map;
use youcraft_api::routes;

use crate::lang::tr;

/// The page.
#[component]
pub fn SignInFailed() -> impl IntoView {
    let query = use_query_map();
    let why = move || query.read().get("why").unwrap_or_default();
    let next = move || {
        query
            .read()
            .get("next")
            .unwrap_or_else(|| routes::HOME.to_owned())
    };
    let text = move || {
        if why() == routes::SIGN_IN_UNAVAILABLE {
            tr("Sign-in is not set up on this server.")
        } else {
            tr("Google did not confirm who you are, or signing in was cancelled.")
        }
    };
    view! {
        <Title text=format!("{} - YouCraft", tr("Sign-in did not work"))/>
        <main class="trouble">
            <h1>{tr("Sign-in did not work")}</h1>
            <p>{text}</p>
            <a class="pill" href=move || routes::sign_in(routes::local_path(&next()))>{tr("Try again")}</a>
        </main>
    }
}
