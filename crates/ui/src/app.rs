//! The application: the router that picks the page, and around every page but the embedded
//! player, the masthead and the live region of notes.
//!
//! The server renders each page whole once its data has come ([`SsrMode::Async`]): its status
//! (404, 401, 403) and its title come from that data, and they belong in the response itself,
//! for browsers, link previews and search engines, not in a script that streams after it.

use leptos::prelude::*;
use leptos_meta::{Title, provide_meta_context};
use leptos_router::components::{Outlet, ParentRoute, Route, Router, Routes};
use leptos_router::{SsrMode, path};

use crate::account::{Shown, SignedIn};
use crate::components::masthead::Masthead;
use crate::lang::theme;
use crate::notice::{NoticeRegion, Notices};
use crate::pages::account::Account;
use crate::pages::admin::Admin;
use crate::pages::channel::{Channel, Tab};
use crate::pages::customize::Customize;
use crate::pages::embed::Embed;
use crate::pages::history::History;
use crate::pages::home::Home;
use crate::pages::not_found::NotFound;
use crate::pages::playlist::Playlist;
use crate::pages::playlists::Playlists;
use crate::pages::results::Results;
use crate::pages::sign_in_failed::SignInFailed;
use crate::pages::studio::Studio;
use crate::pages::subscriptions::Subscriptions;
use crate::pages::upload::UploadPage;
use crate::pages::watch::Watch;

/// The title of a page named `name`, as the browser's tab shows it.
pub fn titled(name: &str) -> String {
    format!("{name} - YouCraft")
}

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
    view! {
        <Title text="YouCraft"/>
        <Router>
            <Routes fallback=move || view! { <Site guide><NotFound/></Site> }>
                <Route path=path!("/embed/:v") ssr=SsrMode::Async view=Embed/>
                <ParentRoute path=path!("") ssr=SsrMode::Async view=move || view! { <Site guide><Outlet/></Site> }>
                    <Route path=path!("") view=Home/>
                    <Route path=path!("/watch") view=Watch/>
                    <Route path=path!("/account") view=Account/>
                    <Route path=path!("/signin/failed") view=SignInFailed/>
                    <Route path=path!("/studio") view=Studio/>
                    <Route path=path!("/studio/customize") view=Customize/>
                    <Route path=path!("/admin") view=Admin/>
                    <Route path=path!("/upload") view=UploadPage/>
                    <Route path=path!("/results") view=Results/>
                    <Route path=path!("/feed/subscriptions") view=Subscriptions/>
                    <Route path=path!("/feed/history") view=History/>
                    <Route path=path!("/feed/playlists") view=Playlists/>
                    <Route path=path!("/playlist") view=Playlist/>
                    <Route path=path!("/:handle") view=|| view! { <Channel tab=Tab::Home/> }/>
                    <Route path=path!("/:handle/videos") view=|| view! { <Channel tab=Tab::Videos/> }/>
                    <Route path=path!("/:handle/playlists") view=|| view! { <Channel tab=Tab::Playlists/> }/>
                </ParentRoute>
            </Routes>
        </Router>
    }
}

/// The site around a page: the masthead, the guide, open as `guide` says, who is signed in,
/// which only the site asks (the embedded player renders without waiting on it), and the live
/// region where the notes of what people do appear.
#[component]
fn Site(guide: GuideOpen, children: Children) -> impl IntoView {
    SignedIn::provide();
    let notices = Notices::provide();
    view! {
        <div class="app" class:guide-collapsed=move || !guide.0.get()>
            <Masthead/>
            {children()}
            <NoticeRegion notices/>
        </div>
    }
}
