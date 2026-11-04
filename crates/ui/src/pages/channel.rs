//! A channel's page, `/@handle`: its avatar, name, handle and counts, Subscribe, then its tabs:
//! Home with its latest videos, Videos with all of them, Playlists with its public lists.

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_params_map;
use youcraft_api::commands::channel::{self, ChannelPage, Handle, Shelf};
use youcraft_api::commands::playlist::PlaylistCard;
use youcraft_api::records::VideoCard;
use youcraft_api::{ApiError, ChannelId, routes};

use crate::call::call;
use crate::components::avatar::{Face, Size};
use crate::components::card::{GridCard, PlaylistGridCard};
use crate::components::guide::Guide;
use crate::components::more::{Pages, ShowMore};
use crate::components::subscribe::SubscribeButton;
use crate::lang::{compact_count, tr};
use crate::pages::not_found::Trouble;

/// The tabs of a channel's page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    /// Its latest videos.
    Home,
    /// All its videos, newest first.
    Videos,
    /// Its public playlists.
    Playlists,
}

/// The handle the address names, `/@handle`, if its first segment starts with `@`.
fn handle() -> Memo<Option<String>> {
    let params = use_params_map();
    Memo::new(move |_| {
        params
            .read()
            .get("handle")
            .and_then(|segment| segment.strip_prefix('@').map(str::to_owned))
    })
}

/// A channel's page, open on `tab`.
#[component]
pub fn Channel(tab: Tab) -> impl IntoView {
    let handle = handle();
    let page = Resource::new(
        move || handle.get(),
        |handle| async move {
            match handle {
                Some(handle) => call::<channel::Get>(Handle { handle }).await,
                None => Err(ApiError::NotFound),
            }
        },
    );
    view! {
        <div class="with-guide">
            <Guide/>
            <main class="browse channel-page">
                <Suspense fallback=|| ()>
                    {move || Suspend::new(async move {
                        match page.await {
                            Ok(page) => view! { <Heading page=page.clone()/><Tabs page tab/> }.into_any(),
                            Err(ApiError::NotFound) => view! { <crate::pages::not_found::NotFound/> }.into_any(),
                            Err(error) => view! { <Trouble error/> }.into_any(),
                        }
                    })}
                </Suspense>
            </main>
        </div>
    }
}

/// The top of the page.
#[component]
fn Heading(page: ChannelPage) -> impl IntoView {
    let subscribers = RwSignal::new(page.subscribers);
    let videos = compact_count(page.videos, "{n} video", "{n} videos");
    view! {
        <Title text=format!("{} - YouCraft", page.card.name)/>
        <header class="channel-heading">
            <span class="avatar avatar-channel" aria-hidden="true"><Face channel=page.card.clone() size=Size::Huge/></span>
            <div class="channel-names">
                <h1>{page.card.name.clone()}</h1>
                <p class="byline">
                    <bdi class="channel-handle">{format!("@{}", page.card.handle)}</bdi>
                    " • "
                    {move || compact_count(subscribers.get(), "{n} subscriber", "{n} subscribers")}
                    " • "
                    {videos}
                </p>
                {(!page.description.is_empty()).then(|| view! {
                    <p class="byline description-line">{page.description.clone()}</p>
                })}
                <div class="channel-buttons">
                    <SubscribeButton channel=page.card.id subscribed=page.subscribed subscribers owned=page.owned/>
                    {page.owned.then(|| view! {
                        <a class="pill" href=routes::STUDIO_CUSTOMIZE>{tr("Customize channel")}</a>
                        <a class="pill" href=routes::STUDIO>{tr("Manage videos")}</a>
                    })}
                </div>
            </div>
        </header>
    }
}

/// The tabs, and the one open.
#[component]
fn Tabs(page: ChannelPage, tab: Tab) -> impl IntoView {
    let handle = page.card.handle.clone();
    let link = move |to: Tab, label: &'static str| {
        let href = match to {
            Tab::Home => routes::channel(&handle),
            Tab::Videos => routes::channel_tab(&handle, "videos"),
            Tab::Playlists => routes::channel_tab(&handle, "playlists"),
        };
        view! {
            <a class="tab" class:current=tab == to href=href aria-current=(tab == to).then_some("page")>{label}</a>
        }
    };
    let channel = page.card.id;
    view! {
        <nav class="tabs" aria-label=tr("Channel")>
            {link(Tab::Home, tr("Home"))}
            {link(Tab::Videos, tr("Videos"))}
            {link(Tab::Playlists, tr("Playlists"))}
        </nav>
        {match tab {
            Tab::Home | Tab::Videos => view! { <VideoShelf channel full=tab == Tab::Videos/> }.into_any(),
            Tab::Playlists => view! { <PlaylistShelf channel/> }.into_any(),
        }}
    }
}

/// The channel's videos, newest first: the first ones on Home, page after page on Videos.
#[component]
fn VideoShelf(channel: ChannelId, full: bool) -> impl IntoView {
    let first = Resource::new(
        move || channel,
        |channel| {
            call::<channel::Videos>(Shelf {
                channel,
                after: None,
            })
        },
    );
    view! {
        <Suspense fallback=|| ()>
            {move || Suspend::new(async move {
                match first.await {
                    Ok(feed) if feed.videos.is_empty() => view! {
                        <p class="empty">{tr("This channel has no videos yet.")}</p>
                    }.into_any(),
                    Ok(feed) => {
                        let pages: Pages<VideoCard> = Pages::new(feed.videos, if full { feed.next } else { None });
                        let more = Callback::new(move |()| pages.read_next::<channel::Videos>(
                            |after| Shelf { channel, after },
                            |feed| (feed.videos, feed.next),
                        ));
                        view! {
                            <div class="grid">
                                <For each=move || pages.items.get() key=|card| card.id let:card>
                                    <GridCard card/>
                                </For>
                            </div>
                            <ShowMore next=pages.next more/>
                        }.into_any()
                    }
                    Err(error) => view! { <Trouble error/> }.into_any(),
                }
            })}
        </Suspense>
    }
}

/// The channel's public playlists.
#[component]
fn PlaylistShelf(channel: ChannelId) -> impl IntoView {
    let first = Resource::new(
        move || channel,
        |channel| {
            call::<channel::Playlists>(Shelf {
                channel,
                after: None,
            })
        },
    );
    view! {
        <Suspense fallback=|| ()>
            {move || Suspend::new(async move {
                match first.await {
                    Ok(shelf) if shelf.playlists.is_empty() => view! {
                        <p class="empty">{tr("This channel has no playlists.")}</p>
                    }.into_any(),
                    Ok(shelf) => {
                        let pages: Pages<PlaylistCard> = Pages::new(shelf.playlists, shelf.next);
                        let more = Callback::new(move |()| pages.read_next::<channel::Playlists>(
                            |after| Shelf { channel, after },
                            |shelf| (shelf.playlists, shelf.next),
                        ));
                        view! {
                            <div class="grid">
                                <For each=move || pages.items.get() key=|card| card.list let:card>
                                    <PlaylistGridCard card/>
                                </For>
                            </div>
                            <ShowMore next=pages.next more/>
                        }.into_any()
                    }
                    Err(error) => view! { <Trouble error/> }.into_any(),
                }
            })}
        </Suspense>
    }
}
