//! The embedded player, `/embed/<id>`: the video alone, filling the frame another site shows it
//! in, with its title linking to its watch page on YouCraft, as YouTube's embeds do.

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_params_map;
use youcraft_api::commands::video;
use youcraft_api::records::VideoPage;
use youcraft_api::{ApiError, VideoId, routes};
use youcraft_player::VideoPlayer;

use crate::act::ask;
use crate::app::titled;
use crate::call::call;
use crate::lang::tr;
use crate::notice::describe;
use crate::pages::not_found::answer_with_status_of;

/// The video the address names.
fn chosen() -> Memo<Option<VideoId>> {
    let params = use_params_map();
    Memo::new(move |_| params.read().get("v").and_then(|v| v.parse().ok()))
}

/// The embedded player page.
#[component]
pub fn Embed() -> impl IntoView {
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
    view! {
        <main class="embed">
            <Suspense fallback=|| view! { <div class="player"></div> }>
                {move || Suspend::new(async move {
                    match page.await {
                        Ok(page) => view! { <Embedded page/> }.into_any(),
                        Err(error) => {
                            answer_with_status_of(&error);
                            view! {
                                <div class="player"><p class="unavailable">{describe(&error)}</p></div>
                            }
                            .into_any()
                        }
                    }
                })}
            </Suspense>
        </main>
    }
}

/// The player of `page`'s video, under its title.
#[component]
fn Embedded(page: VideoPage) -> impl IntoView {
    let card = page.card;
    let video = card.id;
    let started = Callback::new(move |()| ask::<video::View>(video::Which { video }, |_| ()));
    let title = view! {
        <a class="embed-title" href=routes::watch(video) target="_blank" rel="noopener" dir="auto">
            {card.title.clone()}
        </a>
    };
    view! {
        <Title text=titled(&card.title)/>
        {if page.playable {
            view! {
                <VideoPlayer
                    video
                    duration=card.duration
                    poster=routes::thumbnail(video, 720)
                    started
                />
            }
            .into_any()
        } else {
            view! { <div class="player"><p class="unavailable">{tr("This video isn't available yet.")}</p></div> }.into_any()
        }}
        {title}
    }
}
