//! The row under a watched video's title: its channel with the Subscribe button, then the
//! video's actions: like and dislike, share, save to a list.

use leptos::prelude::*;
use youcraft_api::commands::video::{Rate, Rated, SetRating};
use youcraft_api::records::{Rating, VideoPage};
use youcraft_api::{VideoId, routes};

use crate::act::Optimistic;
use crate::browser;
use crate::components::avatar::{Face, Size};
use crate::components::save::SaveDialog;
use crate::components::subscribe::{SubscribeButton, subscription};
use crate::icons::{Glyph, Icon};
use crate::lang::{compact, compact_count, tr};
use crate::notice::Notices;

/// The row.
#[component]
pub fn OwnerRow(page: VideoPage) -> AnyView {
    let card = page.card;
    let channel = routes::channel(&card.channel.handle);
    let state = subscription(page.subscribed, page.subscribers);
    view! {
        <div class="owner-row">
            <div class="owner">
                <a class="avatar" href=channel.clone() aria-hidden="true" tabindex="-1"><Face channel=card.channel.clone() size=Size::Regular/></a>
                <div class="owner-names">
                    <a class="owner-name" href=channel>{card.channel.name.clone()}</a>
                    <span class="owner-subscribers">
                        {move || compact_count(state.shown.get().subscribers, "{n} subscriber", "{n} subscribers")}
                    </span>
                </div>
                <SubscribeButton channel=card.channel.id state owned=page.owned/>
            </div>
            <div class="actions">
                <RatingButtons video=card.id likes=page.likes rating=page.rating/>
                <ShareButton video=card.id/>
                <SaveButton video=card.id/>
            </div>
        </div>
    }
    .into_any()
}

/// The likes a video shows while the change from `before` to `after` is on its way.
fn likes_after(likes: u64, before: Rating, after: Rating) -> u64 {
    let likes = if before == Rating::Like {
        likes.saturating_sub(1)
    } else {
        likes
    };
    if after == Rating::Like {
        likes.saturating_add(1)
    } else {
        likes
    }
}

/// The like and dislike buttons, joined in one pill; pressing the pressed one takes it back.
#[component]
fn RatingButtons(video: VideoId, likes: u64, rating: Rating) -> impl IntoView {
    let state = Optimistic::new(Rated { likes, rating });
    let rate = move |pressed: Rating| {
        let now = state.shown.get_untracked();
        let after = if now.rating == pressed {
            Rating::None
        } else {
            pressed
        };
        let wanted = Rated {
            rating: after,
            likes: likes_after(now.likes, now.rating, after),
        };
        state.change::<SetRating>(
            wanted,
            Rate {
                video,
                rating: after,
            },
            |rated| rated,
        );
    };
    let pressed = move |which: Rating| {
        if state.shown.get().rating == which {
            "true"
        } else {
            "false"
        }
    };
    view! {
        <span class="pill like-pill">
            <button class="pill-part" type="button" aria-label=tr("Like")
                aria-pressed=move || pressed(Rating::Like) on:click=move |_| rate(Rating::Like)>
                <Glyph icon=Icon::Like/><span>{move || compact(state.shown.get().likes)}</span>
            </button>
            <button class="pill-part" type="button" aria-label=tr("Dislike")
                aria-pressed=move || pressed(Rating::Dislike) on:click=move |_| rate(Rating::Dislike)>
                <Glyph icon=Icon::Dislike/>
            </button>
        </span>
    }
}

/// Share: the video's address goes to the clipboard, and a note says so.
#[component]
fn ShareButton(video: VideoId) -> impl IntoView {
    let share = move |_| {
        browser::copy(&format!("{}{}", browser::origin(), routes::watch(video)));
        Notices::say_here(tr("Link copied").to_owned());
    };
    view! {
        <button class="pill" type="button" on:click=share><Glyph icon=Icon::Share/><span>{tr("Share")}</span></button>
    }
}

/// Save: the dialog that puts the video in the person's lists.
#[component]
fn SaveButton(video: VideoId) -> impl IntoView {
    let open = RwSignal::new(false);
    view! {
        <button class="pill" type="button" aria-haspopup="dialog" on:click=move |_| open.set(true)>
            <Glyph icon=Icon::Save/><span>{tr("Save")}</span>
        </button>
        <Show when=move || open.get()>
            <SaveDialog video open/>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn likes_move_with_the_rating_in_advance() {
        assert_eq!(likes_after(5, Rating::None, Rating::Like), 6);
        assert_eq!(likes_after(5, Rating::Like, Rating::None), 4);
        assert_eq!(likes_after(5, Rating::Like, Rating::Dislike), 4);
        assert_eq!(likes_after(0, Rating::Like, Rating::None), 0);
        assert_eq!(likes_after(5, Rating::Dislike, Rating::Like), 6);
    }
}
