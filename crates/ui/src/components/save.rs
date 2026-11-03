//! The Save dialog of a video: a tick for each of the person's lists that holds it, ticking or
//! unticking puts it in or takes it out, and a new playlist can be made with it inside.

use leptos::prelude::*;
use youcraft_api::commands::playlist::{
    Add, Create, Item, Lists, Mine, NewPlaylist, PlaylistCard, Remove,
};
use youcraft_api::records::Visibility;
use youcraft_api::{ApiError, List, VideoId, routes};

use crate::act::act;
use crate::browser;
use crate::call::call;
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

/// The name of a visibility.
pub fn visibility_name(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Public => tr("Public"),
        Visibility::Unlisted => tr("Unlisted"),
        Visibility::Private => tr("Private"),
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
    let held = RwSignal::new(card.contains);
    let list = card.list;
    let toggle = move |_| {
        let wanted = !held.get_untracked();
        held.set(wanted);
        let item = Item { list, video };
        let undo = move |answer: Result<PlaylistCard, ApiError>| match answer {
            Ok(card) => held.set(card.contains),
            Err(_) => held.set(!wanted),
        };
        if wanted {
            act::<Add>(item, undo);
        } else {
            act::<Remove>(item, undo);
        }
    };
    let private = card.visibility == Visibility::Private;
    view! {
        <label class="dialog-entry">
            <input type="checkbox" prop:checked=move || held.get() on:change=toggle/>
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
    let choose = move |event: leptos::ev::Event| {
        let chosen = Visibility::from_name(&event_target_value(&event)).unwrap_or_default();
        visibility.set(chosen);
    };
    let options = [Visibility::Public, Visibility::Unlisted, Visibility::Private].map(|v| {
        view! { <option value=v.as_str() selected=v == Visibility::Private>{visibility_name(v)}</option> }
    });
    view! {
        <Show when=move || making.get() fallback=move || view! {
            <button class="dialog-new" type="button" on:click=move |_| making.set(true)>
                <Glyph icon=Icon::Plus/><span>{tr("New playlist")}</span>
            </button>
        }>
            <form class="dialog-form" on:submit=create>
                <input class="field" type="text" required maxlength="150" placeholder=tr("Choose a title")
                    aria-label=tr("Title") prop:value=move || title.get()
                    on:input=move |event| title.set(event_target_value(&event))/>
                <select class="field" aria-label=tr("Visibility") on:change=choose>
                    {options.clone().into_iter().collect_view()}
                </select>
                <div class="dialog-buttons">
                    <button class="pill quiet" type="button" on:click=move |_| making.set(false)>{tr("Cancel")}</button>
                    <button class="pill primary" type="submit">{tr("Create")}</button>
                </div>
            </form>
        </Show>
    }
}
