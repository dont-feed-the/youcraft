//! The watch history, `/feed/history`: what the person watched, the most recent first, each
//! with a way to take it out; and beside it, the ways to clear the history and to pause it.

use leptos::prelude::*;
use leptos_meta::Title;
use youcraft_api::commands::feed::Page;
use youcraft_api::commands::history::{self, Pausing, Watched};
use youcraft_api::commands::site::Nothing;
use youcraft_api::commands::video::Which;

use crate::act::act;
use crate::app::titled;
use crate::call::call;
use crate::components::card::RowCard;
use crate::components::guide::Guide;
use crate::components::more::{Pages, ShowMore};
use crate::icons::{Glyph, Icon};
use crate::lang::tr;
use crate::pages::not_found::Trouble;

/// The history page.
#[component]
pub fn History() -> impl IntoView {
    let first = Resource::new(|| (), |()| call::<history::List>(Page::default()));
    view! {
        <Title text=titled(tr("History"))/>
        <div class="with-guide">
            <Guide/>
            <main class="browse">
                <h1 class="browse-title">{tr("Watch history")}</h1>
                <Suspense fallback=|| ()>
                    {move || Suspend::new(async move {
                        match first.await {
                            Ok(history) => view! { <Watches history/> }.into_any(),
                            Err(error) => view! { <Trouble error/> }.into_any(),
                        }
                    })}
                </Suspense>
            </main>
        </div>
    }
}

/// The videos, and the history's controls.
#[component]
fn Watches(history: history::History) -> impl IntoView {
    let pages: Pages<Watched> = Pages::new(history.videos, history.next);
    let paused = RwSignal::new(history.paused);
    let more = Callback::new(move |()| {
        pages.read_next::<history::List>(
            |after| Page { after },
            |history| (history.videos, history.next),
        );
    });
    let clear = move |_| {
        act::<history::Clear>(Nothing {}, move |answer| {
            if answer.is_ok() {
                pages.items.set(Vec::new());
                pages.next.set(None);
            }
        });
    };
    let pause = move |_| {
        let wanted = !paused.get_untracked();
        act::<history::Pause>(Pausing { paused: wanted }, move |answer| {
            if let Ok(now) = answer {
                paused.set(now.paused);
            }
        });
    };
    view! {
        <div class="history">
            <div class="rows">
                <Show when=move || pages.items.with(Vec::is_empty)>
                    <p class="empty">{tr("The videos you watch will show here.")}</p>
                </Show>
                <For each=move || pages.items.get() key=|watched| watched.card.id let:watched>
                    <Entry watched pages/>
                </For>
                <ShowMore pages more/>
            </div>
            <aside class="history-controls">
                <button class="menu-entry" type="button" on:click=clear>
                    <Glyph icon=Icon::Delete/><span>{tr("Clear all watch history")}</span>
                </button>
                <button class="menu-entry" type="button" aria-pressed=move || if paused.get() { "true" } else { "false" } on:click=pause>
                    <Glyph icon=Icon::History/>
                    <span>{move || if paused.get() { tr("Turn on watch history") } else { tr("Pause watch history") }}</span>
                </button>
                <Show when=move || paused.get()>
                    <p class="settings-hint">{tr("Your history is paused: what you watch now is not kept.")}</p>
                </Show>
            </aside>
        </div>
    }
}

/// One video of the history, with the way to take it out.
#[component]
fn Entry(watched: Watched, pages: Pages<Watched>) -> impl IntoView {
    let video = watched.card.id;
    let remove = move |_| {
        act::<history::Remove>(Which { video }, move |answer| {
            if answer.is_ok() {
                pages.remove(&video);
            }
        });
    };
    view! {
        <RowCard card=watched.card>
            <button class="icon-button" type="button" aria-label=tr("Remove from watch history") on:click=remove>
                <Glyph icon=Icon::Close/>
            </button>
        </RowCard>
    }
}
