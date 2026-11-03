//! The application: the masthead on every page, and the router that picks the page.

use leptos::prelude::*;
use leptos_meta::{Title, provide_meta_context};
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use crate::account::{Shown, SignedIn};
use crate::components::masthead::Masthead;
use crate::lang::theme;
use crate::pages::account::Account;
use crate::pages::home::Home;
use crate::pages::not_found::NotFound;
use crate::pages::sign_in_failed::SignInFailed;
use crate::pages::studio::Studio;
use crate::pages::upload::UploadPage;
use crate::pages::watch::Watch;

/// Whether the guide (the list of places on the side) is expanded.
#[derive(Clone, Copy)]
pub struct GuideOpen(pub RwSignal<bool>);

/// The whole application.
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    let guide = GuideOpen(RwSignal::new(true));
    provide_context(guide);
    provide_context(Shown(RwSignal::new(theme())));
    SignedIn::provide();
    view! {
        <Title text="YouCraft"/>
        <Router>
            <div class="app" class:guide-collapsed=move || !guide.0.get()>
                <Masthead/>
                <Routes fallback=|| view! { <NotFound/> }>
                    <Route path=path!("") view=Home/>
                    <Route path=path!("/watch") view=Watch/>
                    <Route path=path!("/account") view=Account/>
                    <Route path=path!("/signin/failed") view=SignInFailed/>
                    <Route path=path!("/studio") view=Studio/>
                    <Route path=path!("/upload") view=UploadPage/>
                </Routes>
            </div>
        </Router>
    }
}
