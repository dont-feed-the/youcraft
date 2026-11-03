//! The guide: the places of the site, down the side on wide screens and along the bottom on
//! phones.

use leptos::prelude::*;
use youcraft_api::routes;

use crate::icons::{Glyph, Icon};
use crate::lang::tr;

/// One place the guide leads to, its label already in the page's language.
#[component]
fn Entry(href: &'static str, icon: Icon, label: &'static str) -> impl IntoView {
    view! {
        <a class="guide-entry" href=href>
            <Glyph icon=icon/>
            <span class="guide-label">{label}</span>
        </a>
    }
}

/// The guide.
#[component]
pub fn Guide() -> impl IntoView {
    view! {
        <nav class="guide" aria-label=tr("Guide")>
            <div class="guide-section">
                <Entry href=routes::HOME icon=Icon::Home label=tr("Home")/>
                <Entry href=routes::SUBSCRIPTIONS icon=Icon::Subscriptions label=tr("Subscriptions")/>
            </div>
            <div class="guide-section guide-you">
                <Entry href=routes::PLAYLISTS icon=Icon::You label=tr("You")/>
                <Entry href=routes::HISTORY icon=Icon::History label=tr("History")/>
                <Entry href=routes::PLAYLISTS icon=Icon::Playlists label=tr("Playlists")/>
                <Entry href="/playlist?list=WL" icon=Icon::WatchLater label=tr("Watch later")/>
                <Entry href="/playlist?list=LL" icon=Icon::Like label=tr("Liked videos")/>
            </div>
        </nav>
    }
}
