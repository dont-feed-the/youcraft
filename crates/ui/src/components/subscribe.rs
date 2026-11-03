//! The Subscribe button of a channel, on its page and under its videos: dark until the person
//! subscribes, then quiet and labelled Subscribed, as on YouTube.

use leptos::prelude::*;
use youcraft_api::ChannelId;
use youcraft_api::commands::channel::{Subscribe, Subscribing};

use crate::act::act;
use crate::lang::tr;

/// The button; `subscribers` follows what the site answers. A channel's own owner sees none.
#[component]
pub fn SubscribeButton(
    channel: ChannelId,
    subscribed: bool,
    subscribers: RwSignal<u64>,
    owned: bool,
) -> impl IntoView {
    let subscribed = RwSignal::new(subscribed);
    let toggle = move |_| {
        let wanted = !subscribed.get_untracked();
        subscribed.set(wanted);
        act::<Subscribe>(
            Subscribing {
                channel,
                subscribed: wanted,
            },
            move |answer| match answer {
                Ok(now) => {
                    subscribed.set(now.subscribed);
                    subscribers.set(now.subscribers);
                }
                Err(_) => subscribed.set(!wanted),
            },
        );
    };
    (!owned).then(|| {
        view! {
            <button class="pill subscribe" class:subscribed=move || subscribed.get() type="button"
                aria-pressed=move || if subscribed.get() { "true" } else { "false" } on:click=toggle>
                {move || if subscribed.get() { tr("Subscribed") } else { tr("Subscribe") }}
            </button>
        }
    })
}
