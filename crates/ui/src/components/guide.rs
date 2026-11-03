//! The guide: the places of the site, and the channels the person subscribes to, down the side
//! on wide screens, above a footer with the project's places; a narrow column of icons below
//! 1313px; a tab bar along the bottom on phones.

use leptos::prelude::*;
use youcraft_api::commands::channel::Subscriptions;
use youcraft_api::commands::site::Nothing;
use youcraft_api::{List, routes};

use crate::account::SignedIn;
use crate::call::call;
use crate::components::account_menu::initial;
use crate::components::project;
use crate::icons::{Glyph, Icon};
use crate::lang::tr;

/// One place the guide leads to, its label already in the page's language.
#[component]
fn Entry(href: String, icon: Icon, label: &'static str) -> impl IntoView {
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
                <Entry href=routes::HOME.to_owned() icon=Icon::Home label=tr("Home")/>
                <Entry href=routes::SUBSCRIPTIONS.to_owned() icon=Icon::Subscriptions label=tr("Subscriptions")/>
            </div>
            <div class="guide-section guide-you">
                <Entry href=routes::PLAYLISTS.to_owned() icon=Icon::You label=tr("You")/>
                <Entry href=routes::HISTORY.to_owned() icon=Icon::History label=tr("History")/>
                <Entry href=routes::PLAYLISTS.to_owned() icon=Icon::Playlists label=tr("Playlists")/>
                <Entry href=routes::playlist(List::WatchLater) icon=Icon::WatchLater label=tr("Watch later")/>
                <Entry href=routes::playlist(List::Liked) icon=Icon::Like label=tr("Liked videos")/>
            </div>
            <SubscribedChannels/>
            <footer class="guide-footer">
                {project::places().map(|place| view! {
                    <a class="guide-footer-link" rel="external" href=place.href>{place.label}</a>
                }).into_iter().collect_view()}
            </footer>
        </nav>
    }
}

/// The channels the signed-in person subscribes to, by name.
#[component]
fn SubscribedChannels() -> impl IntoView {
    let me = SignedIn::resource();
    let channels = Resource::new(
        || (),
        move |()| async move {
            let signed_in = match me {
                Some(me) => me.await.is_some(),
                None => false,
            };
            if signed_in {
                call::<Subscriptions>(Nothing {}).await.unwrap_or_default()
            } else {
                Vec::new()
            }
        },
    );
    view! {
        <Suspense fallback=|| ()>
            {move || Suspend::new(async move {
                let channels = channels.await;
                (!channels.is_empty()).then(|| view! {
                    <div class="guide-section guide-channels">
                        <h2 class="guide-heading">{tr("Subscriptions")}</h2>
                        {channels.into_iter().map(|channel| view! {
                            <a class="guide-entry" href=routes::channel(&channel.handle)>
                                <span class="avatar avatar-small" aria-hidden="true">{initial(&channel.name)}</span>
                                <span class="guide-label">{channel.name}</span>
                            </a>
                        }).collect_view()}
                    </div>
                })
            })}
        </Suspense>
    }
}
