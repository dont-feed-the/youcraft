//! The studio of a channel, open to its members, with the way to the studio of their other
//! channels, in three tabs: Content, the channel's videos in every state, page after page, each
//! with its visibility, processing, date and views, and a form to change or delete it, with the
//! way to upload for those who may publish there (those the site does not let publish read that
//! publishing is by invitation); and, for its owners, Customization, in `customize`, and
//! Permissions, in `permissions`. The page's address names the channel; without one, it is the
//! channel the person acts as.
//!
//! While a video is uploading or being processed, the studio asks the site again every few
//! seconds how far it has come, so its progress moves without a reload; each row keeps its own
//! state, an open form included.

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_query_map;
use youcraft_api::commands::account::{Me, Membership};
use youcraft_api::commands::channel::Shelf;
use youcraft_api::commands::{studio, video};
use youcraft_api::records::{Failure, Processing, StudioPage, StudioVideo, Visibility};
use youcraft_api::{ApiError, ChannelId, ChannelRole, Grant, VideoId, routes};

use crate::account::SignedIn;
use crate::act::act;
use crate::app::titled;
use crate::browser::{self, Timer};
use crate::call::call;
use crate::components::avatar::{Face, Size};
use crate::components::more::{Pages, ShowMore};
use crate::components::visibility::{VisibilityChoice, visibility_name};
use crate::icons::{Glyph, Icon};
use crate::lang::{Language, RenderTime, count, fill, tr};
use crate::pages::not_found::Trouble;

/// How long the studio waits before it asks again how far a video has come.
const FOLLOW_EVERY: std::time::Duration = std::time::Duration::from_secs(3);

/// Whether a video in `processing` is still on its way to playing.
pub fn on_its_way(processing: &Processing) -> bool {
    matches!(
        processing,
        Processing::Uploading { .. } | Processing::Processing { .. }
    )
}

/// Where a video is on its way, in words; `None` once it plays.
pub fn processing_text(processing: &Processing) -> Option<String> {
    let percent = |part: u64, whole: u64| {
        let whole_percent = (part.saturating_mul(100) / whole.max(1)).min(100);
        youcraft_i18n::number(Language::current(), whole_percent)
    };
    match processing {
        Processing::Uploading { received, size } => Some(fill(
            tr("Uploading {n}%"),
            &[("n", &percent(*received, *size))],
        )),
        Processing::Processing { permille } => Some(fill(
            tr("Processing {n}%"),
            &[("n", &percent(u64::from(*permille), 1000))],
        )),
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
#[component]
pub fn Studio() -> impl IntoView {
    view! {
        <Title text=titled(tr("Channel content"))/>
        <main class="studio">
            <StudioFrame current=StudioTab::Content body=|me, membership| view! { <Content me membership/> }.into_any()/>
        </main>
    }
}

/// The Content tab of `membership`'s channel, for `me`.
#[component]
fn Content(me: Me, membership: Membership) -> impl IntoView {
    let channel = membership.channel.id;
    let first = Resource::new(
        move || channel,
        |channel| {
            call::<studio::Videos>(Shelf {
                channel,
                after: None,
            })
        },
    );
    // what the account may do, raised to what the site grants every account, and in the channel
    let publishes =
        !me.blocked && me.role >= Grant::Creator && membership.role.allows(ChannelRole::Editor);
    let invited = !me.blocked && me.role < Grant::Creator;
    view! {
        <Suspense fallback=|| ()>
            {move || Suspend::new(async move {
                match first.await {
                    Ok(page) => {
                        let held = !page.videos.is_empty();
                        view! {
                            {publishes.then(|| view! {
                                <div class="studio-actions">
                                    <a class="pill accent" href=routes::in_studio(routes::UPLOAD, channel)>
                                        <Glyph icon=Icon::Upload/>
                                        <span>{tr("Upload video")}</span>
                                    </a>
                                </div>
                            })}
                            {invited.then(|| view! {
                                <p class="empty">{tr("Publishing on YouCraft is by invitation.")}</p>
                            })}
                            {(publishes || held).then(|| view! { <Table channel first=page/> })}
                        }.into_any()
                    }
                    Err(error) => view! { <Trouble error/> }.into_any(),
                }
            })}
        </Suspense>
    }
}

/// A tab of the studio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StudioTab {
    /// The channel's videos.
    Content,
    /// The channel's name, description and photo.
    Customization,
    /// Who runs the channel.
    Permissions,
}

impl StudioTab {
    /// Every tab, in the order the studio shows them.
    const ALL: [StudioTab; 3] = [
        StudioTab::Content,
        StudioTab::Customization,
        StudioTab::Permissions,
    ];

    /// The least role in a channel that opens the tab.
    fn opens_to(self) -> ChannelRole {
        match self {
            StudioTab::Content => ChannelRole::Viewer,
            StudioTab::Customization | StudioTab::Permissions => ChannelRole::Owner,
        }
    }

    /// Its page.
    fn page(self) -> &'static str {
        match self {
            StudioTab::Content => routes::STUDIO,
            StudioTab::Customization => routes::STUDIO_CUSTOMIZE,
            StudioTab::Permissions => routes::STUDIO_PERMISSIONS,
        }
    }

    /// Its name.
    fn label(self) -> &'static str {
        match self {
            StudioTab::Content => tr("Content"),
            StudioTab::Customization => tr("Customization"),
            StudioTab::Permissions => tr("Permissions"),
        }
    }
}

/// The name of a role in a channel.
pub fn channel_role_name(role: ChannelRole) -> &'static str {
    match role {
        ChannelRole::Viewer => tr("Viewer"),
        ChannelRole::Editor => tr("Editor"),
        ChannelRole::Manager => tr("Manager"),
        ChannelRole::Owner => tr("Owner"),
    }
}

/// The channel the page of the studio is about, among those `me` belongs to: the one its address
/// names, else the one they act as, else the first they belong to.
fn studied(me: &Me, named: Option<ChannelId>) -> Option<Membership> {
    let wanted = named.or(me.channel.as_ref().map(|card| card.id));
    me.channels
        .iter()
        .find(|membership| Some(membership.channel.id) == wanted)
        .or_else(|| named.is_none().then(|| me.channels.first()).flatten())
        .cloned()
}

/// A page of the studio: who is signed in, the channel it is about with the way to another, its
/// tabs with `current` open, and `body` for that channel when the person's role there opens it.
#[component]
pub fn StudioFrame(current: StudioTab, body: fn(Me, Membership) -> AnyView) -> impl IntoView {
    let me = SignedIn::resource();
    let query = use_query_map();
    let named = move || {
        query.with(|query| {
            query
                .get(routes::STUDIO_CHANNEL)
                .and_then(|channel| channel.parse::<ChannelId>().ok())
        })
    };
    view! {
        <Suspense fallback=|| ()>
            {move || Suspend::new(async move {
                let me = match me {
                    Some(me) => me.await,
                    None => None,
                };
                let Some(me) = me else {
                    return view! { <Trouble error=ApiError::SignInRequired/> }.into_any();
                };
                let Some(membership) = studied(&me, named()) else {
                    let error = if me.channels.is_empty() { ApiError::ChannelRequired } else { ApiError::NotFound };
                    return view! { <Trouble error/> }.into_any();
                };
                let opens = membership.role.allows(current.opens_to());
                view! {
                    <StudioTabs current membership=membership.clone() channels=me.channels.clone()/>
                    {if opens {
                        body(me, membership)
                    } else {
                        view! { <Trouble error=ApiError::Forbidden/> }.into_any()
                    }}
                }.into_any()
            })}
        </Suspense>
    }
}

/// The studio's heading, the channel it is about among `channels` with the way to another, and
/// the tabs `membership` opens, `current` open.
#[component]
fn StudioTabs(
    current: StudioTab,
    membership: Membership,
    channels: Vec<Membership>,
) -> impl IntoView {
    let channel = membership.channel.id;
    let options = channels.into_iter().map(|other| {
        let id = other.channel.id;
        view! { <option value=id.to_string() selected=id == channel>{other.channel.name}</option> }
    });
    let go = move |event: leptos::ev::Event| {
        if let Ok(other) = event_target_value(&event).parse::<ChannelId>() {
            let page = if current.opens_to() == ChannelRole::Viewer {
                current
            } else {
                StudioTab::Content
            };
            browser::go(&routes::in_studio(page.page(), other));
        }
    };
    let tabs = StudioTab::ALL
        .into_iter()
        .filter(|tab| membership.role.allows(tab.opens_to()))
        .map(|tab| {
            let here = current == tab;
            view! {
                <a class="tab" class:current=here href=routes::in_studio(tab.page(), channel)
                    aria-current=here.then_some("page")>{tab.label()}</a>
            }
        })
        .collect_view();
    view! {
        <h1 class="studio-title">{tr("YouCraft Studio")}</h1>
        <div class="studio-channel">
            <span class="avatar" aria-hidden="true"><Face channel=membership.channel.clone() size=Size::Regular/></span>
            <select class="field" aria-label=tr("Channel") on:change=go>{options.collect_view()}</select>
            <span class="settings-hint">{channel_role_name(membership.role)}</span>
        </div>
        <nav class="tabs" aria-label=tr("YouCraft Studio")>{tabs}</nav>
    }
}

/// The table of the studio's videos, one row per video for as long as it is listed, starting
/// with the `first` page.
///
/// The rows are keyed by video, so a video read again moves what its row shows without
/// rebuilding it, and an open form stays open.
#[component]
fn Table(channel: ChannelId, first: StudioPage) -> impl IntoView {
    let pages: Pages<StudioVideo> = Pages::new(first.videos, first.next);
    let more = Callback::new(move |()| {
        pages.read_next::<studio::Videos>(
            move |after| Shelf { channel, after },
            |page| (page.videos, page.next),
        );
    });
    let follow = Timer::new();
    Effect::new(move |_| {
        let busy: Vec<VideoId> = pages.items.with(|videos| {
            videos
                .iter()
                .filter(|v| on_its_way(&v.processing))
                .map(|v| v.card.id)
                .collect()
        });
        if busy.is_empty() {
            follow.stop();
            return;
        }
        follow.after(FOLLOW_EVERY, move || {
            for video in busy {
                pages.read_again::<studio::Video>(video::Which { video }, |found| found);
            }
        });
    });
    let ids = move || {
        pages
            .items
            .with(|videos| videos.iter().map(|v| v.card.id).collect::<Vec<_>>())
    };
    view! {
        <Show
            when=move || pages.items.with(|videos| !videos.is_empty())
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
                            pages.items.with(|videos| videos.iter().find(|v| v.card.id == id).cloned())
                        })
                        pages
                    />
                </For>
            </div>
        </Show>
        <ShowMore pages more/>
    }
}

/// One video of the studio, as it was last read, and its form.
#[component]
fn Row(video: Memo<Option<StudioVideo>>, pages: Pages<StudioVideo>) -> impl IntoView {
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
            {move || video.get_untracked().map(|video| view! { <Editor video pages/> })}
        </Show>
    }
}

/// The fields of a video's details: its title, description and visibility.
#[component]
pub fn DetailFields(
    title: RwSignal<String>,
    description: RwSignal<String>,
    visibility: RwSignal<Visibility>,
) -> impl IntoView {
    view! {
        <label class="labelled">
            <span>{tr("Title")}</span>
            <input class="field" type="text" dir="auto" required prop:value=move || title.get()
                on:input=move |event| title.set(browser::clipped(&event, video::TITLE_LIMIT))/>
        </label>
        <label class="labelled">
            <span>{tr("Description")}</span>
            <textarea class="field" rows="4" dir="auto" prop:value=move || description.get()
                on:input=move |event| description.set(browser::clipped(&event, video::DESCRIPTION_LIMIT))></textarea>
        </label>
        <label class="labelled">
            <span>{tr("Visibility")}</span>
            <VisibilityChoice chosen=visibility/>
        </label>
    }
}

/// The form that changes a video's details, or deletes it; the studio's list shows the outcome.
#[component]
fn Editor(video: StudioVideo, pages: Pages<StudioVideo>) -> impl IntoView {
    let id = video.card.id;
    let title = RwSignal::new(video.card.title.clone());
    let description = RwSignal::new(video.description.clone());
    let visibility = RwSignal::new(video.visibility);
    let comments = RwSignal::new(video.comments_enabled);
    let saving = RwSignal::new(false);
    let save = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        if saving.get_untracked() {
            return;
        }
        let edit = video::Edit {
            video: id,
            title: Some(title.get_untracked()),
            description: Some(description.get_untracked()),
            visibility: Some(visibility.get_untracked()),
            comments_enabled: Some(comments.get_untracked()),
        };
        saving.set(true);
        act::<video::Update>(edit, move |answer| {
            saving.set(false);
            if answer.is_ok() {
                pages.read_again::<studio::Video>(video::Which { video: id }, |found| found);
            }
        });
    };
    let delete = move |_| {
        #[cfg(feature = "hydrate")]
        if crate::transfer::confirm(tr("Delete this video for good?")) {
            act::<video::Delete>(video::Which { video: id }, move |answer| {
                if answer.is_ok() {
                    pages.remove(&id);
                }
            });
        }
    };
    view! {
        <form class="editor" on:submit=save>
            <DetailFields title description visibility/>
            <label class="check">
                <input type="checkbox" bind:checked=comments/>
                <span>{tr("Allow comments")}</span>
            </label>
            <div class="editor-actions">
                <button class="pill danger" type="button" on:click=delete>{tr("Delete")}</button>
                <button class="pill accent" type="submit" disabled=move || saving.get()>{tr("Save")}</button>
            </div>
        </form>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_is_a_whole_percentage_never_above_a_hundred() {
        let uploading = |received, size| processing_text(&Processing::Uploading { received, size });
        assert_eq!(uploading(0, 1000).as_deref(), Some("Uploading 0%"));
        assert_eq!(uploading(999, 1000).as_deref(), Some("Uploading 99%"));
        assert_eq!(uploading(1000, 1000).as_deref(), Some("Uploading 100%"));
        assert_eq!(uploading(5, 0).as_deref(), Some("Uploading 100%"));
        assert_eq!(uploading(u64::MAX, 2).as_deref(), Some("Uploading 100%"));
        let processing = |permille| processing_text(&Processing::Processing { permille });
        assert_eq!(processing(500).as_deref(), Some("Processing 50%"));
        assert_eq!(processing(2000).as_deref(), Some("Processing 100%"));
        assert_eq!(processing_text(&Processing::Ready), None);
        assert!(on_its_way(&Processing::Processing { permille: 0 }));
        assert!(!on_its_way(&Processing::Ready));
    }
}
