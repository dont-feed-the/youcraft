//! The Subscribe button of a channel, on its page and under its videos: dark until the person
//! subscribes, then quiet and labelled Subscribed, as on YouTube. The subscription and the
//! channel's count move at once, and settle on what the site answers.

use leptos::prelude::*;
use youcraft_api::ChannelId;
use youcraft_api::commands::channel::{Subscribe, Subscribing, Subscription};

use crate::act::Optimistic;
use crate::lang::tr;

/// The subscription to a channel as the page shows it, and its count, for the button and
/// whatever shows the count beside it.
pub fn subscription(subscribed: bool, subscribers: u64) -> Optimistic<Subscription> {
    Optimistic::new(Subscription {
        subscribers,
        subscribed,
    })
}

/// The button. A channel's own owner sees none.
#[component]
pub fn SubscribeButton(
    channel: ChannelId,
    state: Optimistic<Subscription>,
    owned: bool,
) -> impl IntoView {
    let toggle = move |_| {
        let now = state.shown.get_untracked();
        let wanted = Subscription {
            subscribed: !now.subscribed,
            subscribers: if now.subscribed {
                now.subscribers.saturating_sub(1)
            } else {
                now.subscribers.saturating_add(1)
            },
        };
        let subscribing = Subscribing {
            channel,
            subscribed: wanted.subscribed,
        };
        state.change::<Subscribe>(wanted, subscribing, |now| now);
    };
    let subscribed = move || state.shown.get().subscribed;
    (!owned).then(|| {
        view! {
            <button class="pill subscribe" class:subscribed=subscribed type="button"
                aria-pressed=move || if subscribed() { "true" } else { "false" } on:click=toggle>
                {move || if subscribed() { tr("Subscribed") } else { tr("Subscribe") }}
            </button>
        }
    })
}
