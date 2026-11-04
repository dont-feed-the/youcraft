//! The studio: a channel's videos in every state, each with its visibility, processing, date
//! and views, and a form to change or delete it.
//!
//! While a video is uploading or being processed, the list refreshes itself every few seconds,
//! so its progress moves without a reload; each row keeps its own state, an open form included.

use leptos::prelude::*;
use leptos_meta::Title;
use youcraft_api::commands::{feed, studio, video};
use youcraft_api::records::{Failure, Processing, StudioPage, StudioVideo, Visibility};
use youcraft_api::routes;

use crate::browser;
use crate::call::call;
use crate::icons::{Glyph, Icon};
use crate::lang::{Language, RenderTime, count, tr};
use crate::pages::not_found::Trouble;

/// How often the list refreshes while a video is on its way.
const REFRESH_MILLISECONDS: u64 = 3000;

/// What `visibility` is called.
pub fn visibility_name(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Public => tr("Public"),
        Visibility::Unlisted => tr("Unlisted"),
        Visibility::Private => tr("Private"),
    }
}

/// Where a video is on its way, in words; `None` once it plays.
pub fn processing_text(processing: &Processing) -> Option<String> {
    let percent = |part: u64, whole: u64| {
        (part.saturating_mul(100) / whole.max(1))
            .min(100)
            .to_string()
    };
    match processing {
        Processing::Uploading { received, size } => {
            Some(tr("Uploading {n}%").replace("{n}", &percent(*received, *size)))
        }
        Processing::Processing { permille } => {
            Some(tr("Processing {n}%").replace("{n}", &percent(u64::from(*permille), 1000)))
        }
        Processing::Ready => None,
        Processing::Failed { reason } => Some(failure_text(*reason).to_owned()),
    }
}

/// Why a video could not be made playable, in words its creator can act on.
fn failure_text(reason: Failure) -> &'static str {
    match reason {
        Failure::Unsupported => tr("This file isn't a video YouCraft can read."),
        Failure::Decode => tr("This file is damaged."),
        Failure::TooLong => tr("This video is longer than YouCraft accepts."),
        Failure::Limits => tr("This video needs more than YouCraft allows. Try a smaller file."),
        Failure::Internal => tr("Processing failed. Upload the video again."),
    }
}

/// The studio page.
///
/// The first page of videos comes with the page; later pages, asked for while a video is on its
/// way or after an edit, replace it in `latest`. The rows are keyed by video, so a newer page
/// moves what each row shows without rebuilding it, and an open form stays open.
#[component]
pub fn Studio() -> impl IntoView {
    let first = Resource::new(|| (), |()| call::<studio::Videos>(feed::Page::default()));
    let latest = RwSignal::new(None::<StudioPage>);
    let refresh = move || {
        leptos::task::spawn_local(async move {
            if let Ok(page) = call::<studio::Videos>(feed::Page::default()).await {
                latest.set(Some(page));
            }
        });
    };
    Effect::new(move |_| {
        let busy = |page: &StudioPage| {
            page.videos.iter().any(|v| {
                matches!(
                    v.processing,
                    Processing::Uploading { .. } | Processing::Processing { .. }
                )
            })
        };
        let shown = latest.with(|page| page.as_ref().map(busy));
        let busy = shown
            .unwrap_or_else(|| first.with(|page| matches!(page, Some(Ok(page)) if busy(page))));
        if busy {
            set_timeout(
                refresh,
                std::time::Duration::from_millis(REFRESH_MILLISECONDS),
            );
        }
    });
    view! {
        <Title text=tr("Channel content")/>
        <main class="studio">
            <div class="studio-header">
                <h1>{tr("Channel content")}</h1>
                <a class="pill accent" href=routes::UPLOAD>
                    <Glyph icon=Icon::Upload/>
                    <span>{tr("Upload video")}</span>
                </a>
            </div>
            <Suspense fallback=|| ()>
                {move || Suspend::new(async move {
                    match first.await {
                        Ok(page) => {
                            let page = Memo::new(move |_| latest.get().unwrap_or_else(|| page.clone()));
                            view! { <Table page refresh/> }.into_any()
                        }
                        Err(error) => view! { <Trouble error/> }.into_any(),
                    }
                })}
            </Suspense>
        </main>
    }
}

/// The table of the studio's videos, one row per video for as long as it is listed.
#[component]
fn Table(
    page: Memo<StudioPage>,
    refresh: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let ids = move || page.with(|page| page.videos.iter().map(|v| v.card.id).collect::<Vec<_>>());
    view! {
        <Show
            when=move || page.with(|page| !page.videos.is_empty())
            fallback=|| view! { <p class="empty">{tr("No videos yet.")}</p> }
        >
            <div class="studio-table" role="table">
                <div class="studio-row studio-head" role="row">
                    <span role="columnheader">{tr("Video")}</span>
                    <span role="columnheader">{tr("Visibility")}</span>
                    <span role="columnheader">{tr("Date")}</span>
                    <span role="columnheader">{tr("Views")}</span>
                </div>
                <For each=ids key=|id| *id let:id>
                    <Row
                        video=Memo::new(move |_| {
                            page.with(|page| page.videos.iter().find(|v| v.card.id == id).cloned())
                        })
                        refresh
                    />
                </For>
            </div>
        </Show>
    }
}

/// One video of the studio, as the newest page shows it, and its form.
#[component]
fn Row(
    video: Memo<Option<StudioVideo>>,
    refresh: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let editing = RwSignal::new(false);
    let shown = move |pick: fn(&StudioVideo) -> String| {
        video.with(|v| v.as_ref().map(pick).unwrap_or_default())
    };
    let watch = move || shown(|v| routes::watch(v.card.id));
    let thumbnail = move || {
        video.with(|v| {
            v.as_ref()
                .filter(|v| v.processing == Processing::Ready)
                .map(|v| routes::thumbnail(v.card.id, 168))
        })
    };
    let state = move || video.with(|v| v.as_ref().and_then(|v| processing_text(&v.processing)));
    let date = move || {
        shown(|v| {
            youcraft_i18n::when::ago(
                Language::current(),
                v.card.published_at,
                RenderTime::current(),
            )
        })
    };
    view! {
        <div class="studio-row" role="row">
            <div class="studio-video" role="cell">
                <a class="thumbnail studio-thumb" href=watch tabindex="-1" aria-hidden="true">
                    {move || thumbnail().map(|src| view! { <img src=src alt="" loading="lazy"/> })}
                </a>
                <div class="meta">
                    <a class="title" href=watch>{move || shown(|v| v.card.title.clone())}</a>
                    {move || state().map(|text| view! { <p class="byline studio-state">{text}</p> })}
                    <button class="text-button" type="button" on:click=move |_| editing.update(|e| *e = !*e)>
                        {tr("Edit")}
                    </button>
                </div>
            </div>
            <span role="cell">{move || shown(|v| visibility_name(v.visibility).to_owned())}</span>
            <span role="cell">{date}</span>
            <span role="cell">{move || shown(|v| count(v.card.views, "{n} view", "{n} views"))}</span>
        </div>
        <Show when=move || editing.get()>
            {move || video.get_untracked().map(|video| view! { <Editor video refresh/> })}
        </Show>
    }
}

/// The value a `<select>` gives `visibility`.
fn visibility_value(visibility: Visibility) -> String {
    visibility.as_str().to_owned()
}

/// The form that changes a video's details, or deletes it.
#[component]
pub fn Editor(
    video: StudioVideo,
    refresh: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let id = video.card.id;
    let title = RwSignal::new(video.card.title.clone());
    let description = RwSignal::new(video.description.clone());
    let visibility = RwSignal::new(visibility_value(video.visibility));
    let comments = RwSignal::new(video.comments_enabled);
    let save = Action::new(move |(): &()| {
        let edit = video::Edit {
            video: id,
            title: Some(title.get_untracked()),
            description: Some(description.get_untracked()),
            visibility: Visibility::from_name(&visibility.get_untracked()),
            comments_enabled: Some(comments.get_untracked()),
        };
        async move { call::<video::Update>(edit).await.map(|_| ()) }
    });
    let delete = Action::new(move |(): &()| async move {
        call::<video::Delete>(video::Which { video: id }).await
    });
    Effect::new(move |_| {
        if save.value().get().is_some_and(|done| done.is_ok())
            || delete.value().get().is_some_and(|done| done.is_ok())
        {
            refresh();
        }
    });
    let ask_delete = move |_| {
        #[cfg(feature = "hydrate")]
        if crate::transfer::confirm(tr("Delete this video for good?")) {
            delete.dispatch(());
        }
    };
    let options = [
        Visibility::Public,
        Visibility::Unlisted,
        Visibility::Private,
    ];
    view! {
        <form class="editor" on:submit=move |event| { event.prevent_default(); save.dispatch(()); }>
            <label class="labelled">
                <span>{tr("Title")}</span>
                <input class="field" type="text" required prop:value=move || title.get()
                    on:input=move |event| title.set(browser::clipped(&event, video::TITLE_LIMIT))/>
            </label>
            <label class="labelled">
                <span>{tr("Description")}</span>
                <textarea class="field" rows="4" prop:value=move || description.get()
                    on:input=move |event| description.set(browser::clipped(&event, video::DESCRIPTION_LIMIT))></textarea>
            </label>
            <label class="labelled">
                <span>{tr("Visibility")}</span>
                <select class="field" bind:value=visibility>
                    {options.map(|v| view! { <option value=visibility_value(v)>{visibility_name(v)}</option> }).collect_view()}
                </select>
            </label>
            <label class="check">
                <input type="checkbox" bind:checked=comments/>
                <span>{tr("Allow comments")}</span>
            </label>
            <div class="editor-actions">
                <button class="pill danger" type="button" on:click=ask_delete>{tr("Delete")}</button>
                <button class="pill accent" type="submit" disabled=move || save.pending().get()>{tr("Save")}</button>
            </div>
        </form>
    }
}
