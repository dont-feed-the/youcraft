//! The person's lists, `/feed/playlists`: Watch later, the videos they liked, their playlists.

use leptos::prelude::*;
use leptos_meta::Title;
use youcraft_api::commands::playlist::{Lists, Mine};

use crate::call::call;
use crate::components::card::PlaylistGridCard;
use crate::components::guide::Guide;
use crate::lang::tr;
use crate::pages::not_found::Trouble;

/// The lists page.
#[component]
pub fn Playlists() -> impl IntoView {
    let lists = Resource::new(|| (), |()| call::<Lists>(Mine::default()));
    view! {
        <Title text=format!("{} - YouCraft", tr("Playlists"))/>
        <div class="with-guide">
            <Guide/>
            <main class="browse">
                <h1 class="browse-title">{tr("Playlists")}</h1>
                <Suspense fallback=|| ()>
                    {move || Suspend::new(async move {
                        match lists.await {
                            Ok(lists) => view! {
                                <div class="grid">
                                    {lists.into_iter().map(|card| view! { <PlaylistGridCard card/> }).collect_view()}
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
