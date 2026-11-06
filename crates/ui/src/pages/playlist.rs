//! A list's page, `/playlist?list=…`: beside its videos, its first video's thumbnail, title,
//! owner, count and visibility; its owner may edit or delete it, and move or remove its videos.

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_query_map;
use youcraft_api::commands::playlist::{
    self, DESCRIPTION_LIMIT, Edit, Item, Open, Placing, PlaylistPage, TITLE_LIMIT,
};
use youcraft_api::records::{VideoCard, Visibility};
use youcraft_api::{ApiError, List, VideoId, routes};

use crate::act::act;
use crate::app::titled;
use crate::browser;
use crate::call::call;
use crate::components::card::RowCard;
use crate::components::guide::Guide;
use crate::components::more::{Pages, ShowMore};
use crate::components::save::list_name;
use crate::components::visibility::{VisibilityChoice, visibility_name};
use crate::icons::{Glyph, Icon};
use crate::lang::{count, tr};
use crate::pages::not_found::Trouble;

/// The list the address names.
fn chosen() -> Memo<Option<List>> {
    let query = use_query_map();
    Memo::new(move |_| query.read().get("list").and_then(|list| list.parse().ok()))
}

/// A list's page.
#[component]
pub fn Playlist() -> impl IntoView {
    let chosen = chosen();
    let page = Resource::new(
        move || chosen.get(),
        |list| async move {
            match list {
                Some(list) => call::<playlist::Get>(Open { list, after: None }).await,
                None => Err(ApiError::NotFound),
            }
        },
    );
    view! {
        <div class="with-guide">
            <Guide/>
            <main class="browse">
                <Suspense fallback=|| ()>
                    {move || Suspend::new(async move {
                        match page.await {
                            Ok(page) => view! { <ListView page/> }.into_any(),
                            Err(error) => view! { <Trouble error/> }.into_any(),
                        }
                    })}
                </Suspense>
            </main>
        </div>
    }
}

/// The list's panel and its videos.
#[component]
fn ListView(page: PlaylistPage) -> AnyView {
    let list = page.card.list;
    let editable = page.editable;
    let pages: Pages<VideoCard> = Pages::new(page.videos, page.next);
    let more = Callback::new(move |()| {
        pages.read_next::<playlist::Get>(
            |after| Open { list, after },
            |page| (page.videos, page.next),
        )
    });
    let title = RwSignal::new(list_name(&page.card));
    let description = RwSignal::new(page.description);
    let visibility = RwSignal::new(page.card.visibility);
    let held = RwSignal::new(page.card.videos);
    let poster = page.card.poster_at(720);
    let owner = page.card.owner;
    let editing = RwSignal::new(false);
    view! {
        <Title text=move || titled(&title.get())/>
        <div class="playlist">
            <aside class="playlist-panel">
                <div class="playlist-poster">
                    {poster.map(|poster| view! { <img src=poster alt="" decoding="async"/> })}
                </div>
                <Show when=move || editing.get() fallback=move || view! {
                    <h1 class="playlist-title">{move || title.get()}</h1>
                    <a class="playlist-owner" href=routes::channel(&owner.handle)>{owner.name.clone()}</a>
                    <p class="byline">
                        {move || visibility_name(visibility.get())}
                        " • "
                        {move || count(held.get(), "{n} video", "{n} videos")}
                    </p>
                    <p class="description-text">{move || description.get()}</p>
                }>
                    <EditForm list title description visibility editing/>
                </Show>
                {(editable && matches!(list, List::Playlist(_))).then(|| view! {
                    <PanelButtons list editing/>
                })}
            </aside>
            <div class="rows playlist-rows">
                <Show when=move || pages.items.with(Vec::is_empty)>
                    <p class="empty">{tr("No videos in this list yet.")}</p>
                </Show>
                <For each=move || pages.items.get() key=|card| card.id let:card>
                    <Entry card list editable pages held/>
                </For>
                <ShowMore pages more/>
            </div>
        </div>
    }
    .into_any()
}

/// The owner's buttons under the panel: edit and delete.
#[component]
fn PanelButtons(list: List, editing: RwSignal<bool>) -> impl IntoView {
    let confirming = RwSignal::new(false);
    let delete = move |_| {
        if !confirming.get_untracked() {
            confirming.set(true);
            return;
        }
        if let List::Playlist(id) = list {
            act::<playlist::Delete>(playlist::Which { playlist: id }, |answer| {
                if answer.is_ok() {
                    browser::go(routes::PLAYLISTS);
                }
            });
        }
    };
    view! {
        <div class="playlist-buttons">
            <button class="pill" type="button" on:click=move |_| editing.set(true)>
                <Glyph icon=Icon::Edit/><span>{tr("Edit")}</span>
            </button>
            <button class="pill" type="button" on:click=delete>
                <Glyph icon=Icon::Delete/>
                <span>{move || if confirming.get() { tr("Delete for good") } else { tr("Delete playlist") }}</span>
            </button>
        </div>
    }
}

/// The form that changes a playlist's title, description and visibility.
#[component]
fn EditForm(
    list: List,
    title: RwSignal<String>,
    description: RwSignal<String>,
    visibility: RwSignal<Visibility>,
    editing: RwSignal<bool>,
) -> impl IntoView {
    let draft_title = RwSignal::new(title.get_untracked());
    let draft_description = RwSignal::new(description.get_untracked());
    let draft_visibility = RwSignal::new(visibility.get_untracked());
    let save = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        let List::Playlist(id) = list else {
            return;
        };
        let edit = Edit {
            playlist: id,
            title: Some(draft_title.get_untracked()),
            description: Some(draft_description.get_untracked()),
            visibility: Some(draft_visibility.get_untracked()),
        };
        act::<playlist::Update>(edit, move |answer| {
            if let Ok(card) = answer {
                title.set(card.title);
                visibility.set(card.visibility);
                description.set(draft_description.get_untracked().trim().to_owned());
                editing.set(false);
            }
        });
    };
    view! {
        <form class="dialog-form" on:submit=save>
            <input class="field" type="text" required aria-label=tr("Title")
                prop:value=move || draft_title.get() on:input=move |event| draft_title.set(browser::clipped(&event, TITLE_LIMIT))/>
            <textarea class="field" rows="4" aria-label=tr("Description")
                placeholder=tr("Description") prop:value=move || draft_description.get()
                on:input=move |event| draft_description.set(browser::clipped(&event, DESCRIPTION_LIMIT))></textarea>
            <VisibilityChoice chosen=draft_visibility/>
            <div class="dialog-buttons">
                <button class="pill quiet" type="button" on:click=move |_| editing.set(false)>{tr("Cancel")}</button>
                <button class="pill primary" type="submit">{tr("Save")}</button>
            </div>
        </form>
    }
}

/// One video of the list, with its owner's menu: to the top, to the bottom, out of the list.
#[component]
fn Entry(
    card: VideoCard,
    list: List,
    editable: bool,
    pages: Pages<VideoCard>,
    held: RwSignal<u64>,
) -> impl IntoView {
    let video = card.id;
    let open = RwSignal::new(false);
    let place = move |to_top: bool| {
        open.set(false);
        let position = if to_top { 0 } else { u64::MAX };
        act::<playlist::Move>(
            Placing {
                list,
                video,
                position,
            },
            move |answer| {
                if answer.is_ok() {
                    pages.items.update(|items| move_to(items, video, to_top));
                }
            },
        );
    };
    let remove = move |_| {
        open.set(false);
        act::<playlist::Remove>(Item { list, video }, move |answer| {
            if let Ok(card) = answer {
                held.set(card.videos);
                pages.remove(&video);
            }
        });
    };
    view! {
        <RowCard card>
            {editable.then(|| view! {
                <div class="popup-anchor">
                    <button class="icon-button" type="button" aria-label=tr("Action menu") aria-haspopup="menu"
                        on:click=move |_| open.update(|open| *open = !*open)>
                        <Glyph icon=Icon::More/>
                    </button>
                    <Show when=move || open.get()>
                        <div class="menu-backdrop" on:click=move |_| open.set(false)></div>
                        <div class="popup popup-end" role="menu">
                            <button class="menu-entry" role="menuitem" type="button" on:click=move |_| place(true)>
                                <span>{tr("Move to top")}</span>
                            </button>
                            <button class="menu-entry" role="menuitem" type="button" on:click=move |_| place(false)>
                                <span>{tr("Move to bottom")}</span>
                            </button>
                            <button class="menu-entry" role="menuitem" type="button" on:click=remove>
                                <Glyph icon=Icon::Delete/><span>{tr("Remove from this list")}</span>
                            </button>
                        </div>
                    </Show>
                </div>
            })}
        </RowCard>
    }
}

/// `items` with `video` moved first, or last.
fn move_to(items: &mut Vec<VideoCard>, video: VideoId, to_top: bool) {
    if let Some(at) = items.iter().position(|card| card.id == video) {
        let card = items.remove(at);
        if to_top {
            items.insert(0, card);
        } else {
            items.push(card);
        }
    }
}
