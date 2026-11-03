//! The home page: the guide, then a grid of videos in the order the recommendation chose.

use leptos::prelude::*;
use youcraft_api::commands::feed;

use crate::call::call;
use crate::components::card::GridCard;
use crate::components::guide::Guide;
use crate::lang::tr;
use crate::pages::not_found::Trouble;

/// The home page.
#[component]
pub fn Home() -> impl IntoView {
    let feed = Resource::new(|| (), |()| call::<feed::Home>(feed::Page::default()));
    view! {
        <div class="with-guide">
            <Guide/>
            <main class="browse">
                <Suspense fallback=|| view! { <div class="grid"></div> }>
                    {move || Suspend::new(async move {
                        match feed.await {
                            Ok(feed) if feed.videos.is_empty() => view! {
                                <p class="empty">{tr("No videos yet.")}</p>
                            }.into_any(),
                            Ok(feed) => view! {
                                <div class="grid">
                                    {feed.videos.into_iter().map(|card| view! { <GridCard card/> }).collect_view()}
                                </div>
                            }.into_any(),
                            Err(error) => view! { <Trouble error/> }.into_any(),
                        }
                    })}
                </Suspense>
            </main>
        </div>
    }
}
