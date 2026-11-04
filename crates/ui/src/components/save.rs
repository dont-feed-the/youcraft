//! The Save dialog of a video: a tick for each of the person's lists that holds it, ticking or
//! unticking puts it in or takes it out, and a new playlist can be made with it inside.

use leptos::prelude::*;
use youcraft_api::commands::playlist::{
    Add, Create, Item, Lists, Mine, NewPlaylist, PlaylistCard, Remove, TITLE_LIMIT,
};
use youcraft_api::records::Visibility;
use youcraft_api::{ApiError, List, VideoId, routes};

use crate::act::{Optimistic, act};
use crate::browser;
use crate::call::call;
use crate::components::visibility::VisibilityChoice;
use crate::icons::{Glyph, Icon};
use crate::lang::tr;

/// The name a list goes by: its title, or what the site calls Watch later and liked videos.
pub fn list_name(card: &PlaylistCard) -> String {
    match card.list {
        List::WatchLater => tr("Watch later").to_owned(),
        List::Liked => tr("Liked videos").to_owned(),
        List::Playlist(_) => card.title.clone(),
    }
}

/// The dialog, open while `open` holds.
#[component]
pub fn SaveDialog(video: VideoId, open: RwSignal<bool>) -> impl IntoView {
    let version = RwSignal::new(0_u32);
    let lists = LocalResource::new(move || {
        version.track();
        async move {
            let lists = call::<Lists>(Mine {
                containing: Some(video),
            })
            .await;
            if lists.as_ref().err() == Some(&ApiError::SignInRequired) {
                browser::go(&routes::sign_in(&browser::here()));
            }
            lists.unwrap_or_default()
        }
    });
    let close = move |_| open.set(false);
    let escape = move |event: leptos::ev::KeyboardEvent| {
        if event.key() == "Escape" {
            open.set(false);
        }
    };
    view! {
        <div class="dialog-backdrop" on:click=close></div>
        <div class="dialog" role="dialog" aria-modal="true" aria-label=tr("Save video to…") on:keydown=escape>
            <div class="dialog-heading">
                <h2>{tr("Save video to…")}</h2>
                <button class="icon-button" type="button" aria-label=tr("Close") on:click=close>
                    <Glyph icon=Icon::Close/>
                </button>
            </div>
            <div class="dialog-body">
                <Suspense fallback=|| ()>
                    {move || Suspend::new(async move {
                        lists.await
                            .into_iter()
                            .filter(|card| card.list != List::Liked)
                            .map(|card| view! { <ListTick video card/> })
                            .collect_view()
                    })}
                </Suspense>
            </div>
            <NewPlaylistForm video version/>
        </div>
    }
}

/// One list, ticked if it holds the video.
#[component]
fn ListTick(video: VideoId, card: PlaylistCard) -> impl IntoView {
    let held = Optimistic::new(card.contains);
    let list = card.list;
    let toggle = move |_| {
        let wanted = !held.shown.get_untracked();
        let item = Item { list, video };
        let contains = |card: PlaylistCard| card.contains;
        if wanted {
            held.change::<Add>(wanted, item, contains);
        } else {
            held.change::<Remove>(wanted, item, contains);
        }
    };
    let private = card.visibility == Visibility::Private;
    view! {
        <label class="dialog-entry">
            <input type="checkbox" prop:checked=move || held.shown.get() on:change=toggle/>
            <span class="dialog-entry-name">{list_name(&card)}</span>
            {private.then(|| view! { <span class="dialog-entry-mark" title=tr("Private")><Glyph icon=Icon::Lock/></span> })}
        </label>
    }
}

/// The way to a new playlist holding the video: a button, then its title and visibility.
#[component]
fn NewPlaylistForm(video: VideoId, version: RwSignal<u32>) -> impl IntoView {
    let making = RwSignal::new(false);
    let title = RwSignal::new(String::new());
    let visibility = RwSignal::new(Visibility::Private);
    let create = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        let new = NewPlaylist {
            title: title.get_untracked(),
            visibility: visibility.get_untracked(),
            video: Some(video),
        };
        act::<Create>(new, move |answer| {
            if answer.is_ok() {
                title.set(String::new());
                making.set(false);
                version.update(|v| *v += 1);
            }
        });
    };
    view! {
        <Show when=move || making.get() fallback=move || view! {
            <button class="dialog-new" type="button" on:click=move |_| making.set(true)>
                <Glyph icon=Icon::Plus/><span>{tr("New playlist")}</span>
            </button>
        }>
            <form class="dialog-form" on:submit=create>
                <input class="field" type="text" required placeholder=tr("Choose a title")
                    aria-label=tr("Title") prop:value=move || title.get()
                    on:input=move |event| title.set(browser::clipped(&event, TITLE_LIMIT))/>
                <VisibilityChoice chosen=visibility/>
                <div class="dialog-buttons">
                    <button class="pill quiet" type="button" on:click=move |_| making.set(false)>{tr("Cancel")}</button>
                    <button class="pill primary" type="submit">{tr("Create")}</button>
                </div>
            </form>
        </Show>
    }
}
