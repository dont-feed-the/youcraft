//! The subscriptions feed, `/feed/subscriptions`: the newest videos of the channels the person
//! subscribes to, in a grid.

use leptos::prelude::*;
use leptos_meta::Title;
use youcraft_api::commands::feed::{self, Page};
use youcraft_api::records::VideoCard;

use crate::call::call;
use crate::components::card::GridCard;
use crate::components::guide::Guide;
use crate::components::more::{Pages, ShowMore};
use crate::lang::tr;
use crate::pages::not_found::Trouble;

/// The subscriptions feed.
#[component]
pub fn Subscriptions() -> impl IntoView {
    let first = Resource::new(|| (), |()| call::<feed::Subscriptions>(Page::default()));
    view! {
        <Title text=format!("{} - YouCraft", tr("Subscriptions"))/>
        <div class="with-guide">
            <Guide/>
            <main class="browse">
                <h1 class="browse-title">{tr("Latest")}</h1>
                <Suspense fallback=|| ()>
                    {move || Suspend::new(async move {
                        match first.await {
                            Ok(feed) if feed.videos.is_empty() => view! {
                                <p class="empty">{tr("The new videos of the channels you subscribe to will show here.")}</p>
                            }.into_any(),
                            Ok(feed) => {
                                let pages: Pages<VideoCard> = Pages::new(feed.videos, feed.next);
                                let more = Callback::new(move |()| pages.read_next::<feed::Subscriptions>(
                                    |after| Page { after },
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
            </main>
        </div>
    }
}
