//! The cards of videos and lists: a thumbnail and its badge, then a title, a channel, views and
//! age, in a grid, a narrow column or a wide list.

use leptos::prelude::*;
use youcraft_api::commands::playlist::PlaylistCard;
use youcraft_api::records::VideoCard;
use youcraft_api::routes;

use crate::components::save::{list_name, visibility_name};
use crate::icons::{Glyph, Icon};
use crate::lang::{Language, RenderTime, compact_count, tr};

/// The `srcset` of a video's thumbnail: every width it is made at.
fn thumbnail_set(card: &VideoCard) -> String {
    routes::THUMBNAIL_WIDTHS
        .iter()
        .map(|width| format!("{} {width}w", routes::thumbnail(card.id, *width)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// "1,234 views • 3 days ago", in the page's language, counted from the render time.
fn byline(card: &VideoCard) -> String {
    let views = compact_count(card.views, "{n} view", "{n} views");
    let age = youcraft_i18n::when::ago(
        Language::current(),
        card.published_at,
        RenderTime::current(),
    );
    format!("{views} • {age}")
}

/// A card in a grid, as on the home page.
#[component]
pub fn GridCard(card: VideoCard) -> impl IntoView {
    let watch = routes::watch(card.id);
    let channel = routes::channel(&card.channel.handle);
    let initial = card
        .channel
        .name
        .chars()
        .next()
        .map(String::from)
        .unwrap_or_default();
    view! {
        <article class="card">
            <a class="thumbnail" href=watch.clone() tabindex="-1" aria-hidden="true">
                <img src=routes::thumbnail(card.id, 336) srcset=thumbnail_set(&card)
                    sizes="(max-width: 600px) 100vw, 360px" alt="" loading="lazy" decoding="async"/>
                <span class="duration">{youcraft_i18n::when::clock(card.duration.whole_seconds())}</span>
            </a>
            <div class="details">
                <a class="avatar" href=channel.clone() aria-hidden="true" tabindex="-1">{initial}</a>
                <div class="meta">
                    <h3 class="title" dir="auto"><a href=watch>{card.title.clone()}</a></h3>
                    <a class="channel-name" href=channel>{card.channel.name.clone()}</a>
                    <p class="byline">{byline(&card)}</p>
                </div>
            </div>
        </article>
    }
}

/// A card in a list, as beside the video being watched.
#[component]
pub fn CompactCard(card: VideoCard) -> impl IntoView {
    let watch = routes::watch(card.id);
    view! {
        <article class="compact-card">
            <a class="thumbnail" href=watch.clone() tabindex="-1" aria-hidden="true">
                <img src=routes::thumbnail(card.id, 168) srcset=thumbnail_set(&card) sizes="168px"
                    alt="" loading="lazy" decoding="async"/>
                <span class="duration">{youcraft_i18n::when::clock(card.duration.whole_seconds())}</span>
            </a>
            <div class="meta">
                <h3 class="title" dir="auto"><a href=watch>{card.title.clone()}</a></h3>
                <a class="channel-name" href=routes::channel(&card.channel.handle)>{card.channel.name.clone()}</a>
                <p class="byline">{byline(&card)}</p>
            </div>
        </article>
    }
}

/// A card in a wide list, as in search results, the history and a playlist: a large thumbnail
/// and the details beside it, then whatever the page puts at its end.
#[component]
pub fn RowCard(card: VideoCard, #[prop(optional)] children: Option<Children>) -> impl IntoView {
    let watch = routes::watch(card.id);
    let channel = routes::channel(&card.channel.handle);
    view! {
        <article class="row-card">
            <a class="thumbnail" href=watch.clone() tabindex="-1" aria-hidden="true">
                <img src=routes::thumbnail(card.id, 336) srcset=thumbnail_set(&card)
                    sizes="(max-width: 656px) 160px, 360px" alt="" loading="lazy" decoding="async"/>
                <span class="duration">{youcraft_i18n::when::clock(card.duration.whole_seconds())}</span>
            </a>
            <div class="meta">
                <h3 class="title" dir="auto"><a href=watch>{card.title.clone()}</a></h3>
                <p class="byline">{byline(&card)}</p>
                <a class="channel-name" href=channel>{card.channel.name.clone()}</a>
            </div>
            {children.map(|children| view! { <div class="row-card-end">{children()}</div> })}
        </article>
    }
}

/// A list in a grid: its first video's thumbnail with how many videos it holds, its title and
/// owner, leading to its page.
#[component]
pub fn PlaylistGridCard(card: PlaylistCard) -> impl IntoView {
    let page = routes::playlist(card.list);
    let videos = compact_count(card.videos, "{n} video", "{n} videos");
    let name = list_name(&card);
    view! {
        <article class="card playlist-card">
            <a class="thumbnail stacked" href=page.clone() tabindex="-1" aria-hidden="true">
                {card.first.map(|first| view! {
                    <img src=routes::thumbnail(first, 336) alt="" loading="lazy" decoding="async"/>
                })}
                <span class="duration"><Glyph icon=Icon::Playlists/>{videos}</span>
            </a>
            <div class="meta">
                <h3 class="title"><a href=page.clone()>{name}</a></h3>
                <p class="byline">{visibility_name(card.visibility)}</p>
                <a class="channel-name" href=page>{tr("View full playlist")}</a>
            </div>
        </article>
    }
}
