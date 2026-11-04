//! The watch page: the video, its title, channel, actions and description, and what to watch
//! next beside it (below it on narrow screens). In theatre mode the video spans the page's width
//! and the rest moves under it, as on youtube.com.

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_query_map;
use youcraft_api::commands::video;
use youcraft_api::records::VideoPage;
use youcraft_api::{ApiError, VideoId, routes};
use youcraft_player::VideoPlayer;

use crate::act::act;
use crate::call::call;
use crate::components::card::CompactCard;
use crate::components::comments::CommentSection;
use crate::components::video_actions::OwnerRow;
use crate::lang::{Language, RenderTime, compact_count, tr};
use crate::pages::not_found::Trouble;

/// The video the address names, `/watch?v=…`.
fn chosen() -> Memo<Option<VideoId>> {
    let query = use_query_map();
    Memo::new(move |_| query.read().get("v").and_then(|v| v.parse().ok()))
}

/// The watch page.
#[component]
pub fn Watch() -> impl IntoView {
    let chosen = chosen();
    let page = Resource::new(
        move || chosen.get(),
        |video| async move {
            match video {
                Some(video) => call::<video::Get>(video::Which { video }).await,
                None => Err(ApiError::NotFound),
            }
        },
    );
    let next = Resource::new(
        move || chosen.get(),
        |video| async move {
            match video {
                Some(video) => call::<video::Next>(video::Which { video })
                    .await
                    .map(|feed| feed.videos),
                None => Ok(Vec::new()),
            }
        },
    );
    let theatre = RwSignal::new(false);
    view! {
        <div class="watch" class:theatre=move || theatre.get()>
            <Suspense fallback=|| view! { <div class="stage"><div class="player"></div></div> }>
                {move || Suspend::new(async move {
                    match page.await {
                        Ok(page) => view! { <Stage page=page.clone() theatre/><Primary page/> }.into_any(),
                        Err(error) => view! { <div class="primary"><Trouble error/></div> }.into_any(),
                    }
                })}
            </Suspense>
            <aside class="secondary" aria-label=tr("Up next")>
                <Suspense fallback=|| ()>
                    {move || Suspend::new(async move {
                        next.await.unwrap_or_default().into_iter().map(|card| view! { <CompactCard card/> }).collect_view()
                    })}
                </Suspense>
            </aside>
        </div>
    }
}

/// The video, in its player once it can be played.
#[component]
fn Stage(page: VideoPage, theatre: RwSignal<bool>) -> impl IntoView {
    let card = page.card;
    let video = card.id;
    let started = Callback::new(move |()| act::<video::View>(video::Which { video }, |_| ()));
    view! {
        <div class="stage">
            {if page.playable {
                view! {
                    <VideoPlayer
                        video=card.id
                        duration=card.duration
                        poster=routes::thumbnail(card.id, 720)
                        theatre
                        started
                    />
                }
                .into_any()
            } else {
                view! { <div class="player"><p class="unavailable">{tr("This video isn't available yet.")}</p></div> }.into_any()
            }}
        </div>
    }
}

/// Everything under the video.
#[component]
fn Primary(page: VideoPage) -> impl IntoView {
    let card = page.card.clone();
    let views = compact_count(card.views, "{n} view", "{n} views");
    let age = youcraft_i18n::when::ago(
        Language::current(),
        card.published_at,
        RenderTime::current(),
    );
    view! {
        <Title text=format!("{} - YouCraft", card.title)/>
        <div class="primary">
            <h1 class="watch-title" dir="auto">{card.title.clone()}</h1>
            <OwnerRow page=page.clone()/>
            <section class="description">
                <p class="description-meta">{format!("{views}  {age}")}</p>
                <p class="description-text" dir="auto">{page.description}</p>
            </section>
            <CommentSection video=card.id total=page.comments enabled=page.comments_enabled owned=page.owned/>
        </div>
    }
}
