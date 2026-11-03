//! The watch page: the video, its title, channel, actions and description, and what to watch
//! next beside it (below it on narrow screens).

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_query_map;
use youcraft_api::commands::video;
use youcraft_api::records::VideoPage;
use youcraft_api::{ApiError, VideoId, routes};

use crate::call::call;
use crate::components::card::CompactCard;
use crate::icons::{Glyph, Icon};
use crate::lang::{count, tr};
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
    view! {
        <div class="watch">
            <Suspense fallback=|| view! { <div class="primary"><div class="player"></div></div> }>
                {move || Suspend::new(async move {
                    match page.await {
                        Ok(page) => view! { <Primary page/> }.into_any(),
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

/// The video and everything under it.
#[component]
fn Primary(page: VideoPage) -> impl IntoView {
    let card = page.card;
    let poster = routes::thumbnail(card.id, 720);
    let views = count(card.views, "{n} view", "{n} views");
    let likes = count(page.likes, "{n} like", "{n} likes");
    let comments = count(page.comments, "{n} comment", "{n} comments");
    let initial = card
        .channel
        .name
        .chars()
        .next()
        .map(String::from)
        .unwrap_or_default();
    view! {
        <Title text=format!("{} - YouCraft", card.title)/>
        <div class="primary">
            <div class="player">
                {if page.playable {
                    view! { <video controls preload="metadata" poster=poster src=routes::playlist_of_renditions(card.id)></video> }.into_any()
                } else {
                    view! { <p class="unavailable">{tr("This video isn't available yet.")}</p> }.into_any()
                }}
            </div>
            <h1 class="watch-title">{card.title.clone()}</h1>
            <div class="owner-row">
                <a class="avatar" href=routes::channel(&card.channel.handle) aria-hidden="true" tabindex="-1">{initial}</a>
                <a class="owner-name" href=routes::channel(&card.channel.handle)>{card.channel.name.clone()}</a>
                <button class="pill subscribe" type="button">{tr("Subscribe")}</button>
                <div class="actions">
                    <span class="pill like-pill">
                        <button class="pill-part" type="button" aria-label=tr("Like")><Glyph icon=Icon::Like/><span>{likes}</span></button>
                        <button class="pill-part" type="button" aria-label=tr("Dislike")><Glyph icon=Icon::Dislike/></button>
                    </span>
                    <button class="pill" type="button"><Glyph icon=Icon::Share/><span>{tr("Share")}</span></button>
                </div>
            </div>
            <section class="description">
                <p class="description-meta">{views}</p>
                <p class="description-text">{page.description}</p>
            </section>
            <section class="comments-header">
                {if page.comments_enabled {
                    view! { <h2>{comments}</h2> }.into_any()
                } else {
                    view! { <p>{tr("Comments are turned off.")}</p> }.into_any()
                }}
            </section>
        </div>
    }
}
