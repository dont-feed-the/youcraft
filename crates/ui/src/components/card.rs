//! A video's card: its thumbnail and length, then its title, channel, views and age.

use leptos::prelude::*;
use youcraft_api::records::VideoCard;
use youcraft_api::routes;

use crate::lang::{Language, RenderTime, count};

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
    let views = count(card.views, "{n} view", "{n} views");
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
                    <h3 class="title"><a href=watch>{card.title.clone()}</a></h3>
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
                <h3 class="title"><a href=watch>{card.title.clone()}</a></h3>
                <a class="channel-name" href=routes::channel(&card.channel.handle)>{card.channel.name.clone()}</a>
                <p class="byline">{byline(&card)}</p>
            </div>
        </article>
    }
}
