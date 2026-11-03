//! The studio: a channel's videos in every state, each with its visibility, processing, date
//! and views, and a form to change or delete it.
//!
//! While a video is uploading or being processed, the list refreshes itself every few seconds,
//! so its progress moves without a reload.

use leptos::prelude::*;
use leptos_meta::Title;
use youcraft_api::commands::{feed, studio, video};
use youcraft_api::records::{Processing, StudioVideo, Visibility};
use youcraft_api::routes;

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
        Processing::Failed { reason } => Some(
            match reason.as_str() {
                "unsupported" => tr("This file isn't a video YouCraft can read."),
                "decode" => tr("This file is damaged."),
                _ => tr("Processing failed. Upload the video again."),
            }
            .to_owned(),
        ),
    }
}

/// The studio page.
#[component]
pub fn Studio() -> impl IntoView {
    let videos = Resource::new(|| (), |()| call::<studio::Videos>(feed::Page::default()));
    let refresh = move || videos.refetch();
    Effect::new(move |_| {
        let busy = videos.get().and_then(Result::ok).is_some_and(|page| {
            page.videos.iter().any(|v| {
                matches!(
                    v.processing,
                    Processing::Uploading { .. } | Processing::Processing { .. }
                )
            })
        });
        if busy {
            set_timeout(
                move || videos.refetch(),
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
                    match videos.await {
                        Ok(page) if page.videos.is_empty() => view! {
                            <p class="empty">{tr("No videos yet.")}</p>
                        }.into_any(),
                        Ok(page) => view! {
                            <div class="studio-table" role="table">
                                <div class="studio-row studio-head" role="row">
                                    <span role="columnheader">{tr("Video")}</span>
                                    <span role="columnheader">{tr("Visibility")}</span>
                                    <span role="columnheader">{tr("Date")}</span>
                                    <span role="columnheader">{tr("Views")}</span>
                                </div>
                                {page.videos.into_iter().map(|video| view! { <Row video refresh/> }).collect_view()}
                            </div>
                        }.into_any(),
                        Err(error) => view! { <Trouble error/> }.into_any(),
                    }
                })}
            </Suspense>
        </main>
    }
}

/// One video of the studio, and its form.
#[component]
fn Row(video: StudioVideo, refresh: impl Fn() + Copy + Send + Sync + 'static) -> impl IntoView {
    let editing = RwSignal::new(false);
    let id = video.card.id;
    let ready = video.processing == Processing::Ready;
    let state = processing_text(&video.processing);
    let date = youcraft_i18n::when::ago(
        Language::current(),
        video.card.published_at,
        RenderTime::current(),
    );
    let views = count(video.card.views, "{n} view", "{n} views");
    let thumbnail = ready.then(|| routes::thumbnail(id, 168));
    let title = video.card.title.clone();
    view! {
        <div class="studio-row" role="row">
            <div class="studio-video" role="cell">
                <a class="thumbnail studio-thumb" href=routes::watch(id) tabindex="-1" aria-hidden="true">
                    {thumbnail.map(|src| view! { <img src=src alt="" loading="lazy"/> })}
                </a>
                <div class="meta">
                    <a class="title" href=routes::watch(id)>{title}</a>
                    {state.map(|text| view! { <p class="byline studio-state">{text}</p> })}
                    <button class="text-button" type="button" on:click=move |_| editing.update(|e| *e = !*e)>
                        {tr("Edit")}
                    </button>
                </div>
            </div>
            <span role="cell">{visibility_name(video.visibility)}</span>
            <span role="cell">{date}</span>
            <span role="cell">{views}</span>
        </div>
        <Show when=move || editing.get()>
            <Editor video=video.clone() refresh/>
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
                <input class="field" type="text" maxlength="100" required bind:value=title/>
            </label>
            <label class="labelled">
                <span>{tr("Description")}</span>
                <textarea class="field" rows="4" maxlength="5000" bind:value=description></textarea>
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
