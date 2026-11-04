//! The results page, `/results?search_query=…`: the channels the query names, then the videos
//! it finds, in wide rows, as on YouTube.

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_query_map;
use youcraft_api::commands::channel::ChannelPage;
use youcraft_api::commands::search::{Found, Query, Search};
use youcraft_api::records::VideoCard;
use youcraft_api::routes;

use crate::app::titled;
use crate::call::call;
use crate::components::avatar::{Face, Size};
use crate::components::card::RowCard;
use crate::components::guide::Guide;
use crate::components::more::{Pages, ShowMore};
use crate::components::subscribe::{SubscribeButton, subscription};
use crate::lang::{compact_count, tr};
use crate::pages::not_found::Trouble;

/// The query the address holds.
pub fn searched() -> Memo<String> {
    let query = use_query_map();
    Memo::new(move |_| query.read().get("search_query").unwrap_or_default())
}

/// The results page.
#[component]
pub fn Results() -> impl IntoView {
    let searched = searched();
    let found = Resource::new(
        move || searched.get(),
        |query| async move {
            if query.trim().is_empty() {
                return Ok(Found {
                    channels: Vec::new(),
                    videos: Vec::new(),
                    next: None,
                });
            }
            call::<Search>(Query { query, after: None }).await
        },
    );
    view! {
        <Title text=move || titled(&searched.get())/>
        <div class="with-guide">
            <Guide/>
            <main class="browse results">
                <Suspense fallback=|| ()>
                    {move || Suspend::new(async move {
                        match found.await {
                            Ok(found) if found.channels.is_empty() && found.videos.is_empty() => view! {
                                <p class="empty">{tr("No results found. Try other words.")}</p>
                            }.into_any(),
                            Ok(found) => view! { <Found found query=searched.get_untracked()/> }.into_any(),
                            Err(error) => view! { <Trouble error/> }.into_any(),
                        }
                    })}
                </Suspense>
            </main>
        </div>
    }
}

/// What the search found.
#[component]
fn Found(found: Found, query: String) -> impl IntoView {
    let pages: Pages<VideoCard> = Pages::new(found.videos, found.next);
    let more = Callback::new(move |()| {
        let query = query.clone();
        pages.read_next::<Search>(
            move |after| Query { query, after },
            |found| (found.videos, found.next),
        );
    });
    view! {
        <div class="rows">
            {found.channels.into_iter().map(|channel| view! { <ChannelRow channel/> }).collect_view()}
            <For each=move || pages.items.get() key=|card| card.id let:card>
                <RowCard card/>
            </For>
            <ShowMore pages more/>
        </div>
    }
}

/// A channel among the results: its avatar, name, handle and subscribers, and Subscribe.
#[component]
pub fn ChannelRow(channel: ChannelPage) -> impl IntoView {
    let address = routes::channel(&channel.card.handle);
    let state = subscription(channel.subscribed, channel.subscribers);
    view! {
        <article class="channel-row">
            <a class="channel-row-avatar" href=address.clone() tabindex="-1" aria-hidden="true">
                <span class="avatar avatar-huge"><Face channel=channel.card.clone() size=Size::Huge/></span>
            </a>
            <div class="meta">
                <h3 class="title"><a href=address>{channel.card.name.clone()}</a></h3>
                <p class="byline">
                    <bdi>{format!("@{}", channel.card.handle)}</bdi>
                    " • "
                    {move || compact_count(state.shown.get().subscribers, "{n} subscriber", "{n} subscribers")}
                </p>
                <p class="byline description-line">{channel.description.clone()}</p>
            </div>
            <SubscribeButton channel=channel.card.id state owned=channel.owned/>
        </article>
    }
}
