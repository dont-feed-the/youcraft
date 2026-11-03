//! The player as a page shows it: the `<video>`, the surface that answers clicks and taps, what
//! flashes over the picture, and the controls, wired to the element, the engine and the keyboard.
//!
//! A click plays or pauses and a double click fills the screen, as with a mouse on youtube.com;
//! a tap shows or hides the controls and a double tap on either side jumps ten seconds, as on
//! m.youtube.com. The controls run left to right in every language, as time does; the menu's
//! text follows the page's direction.

use leptos::ev;
use leptos::prelude::*;
use youcraft_api::VideoId;
use youcraft_api::records::Storyboard;
use youcraft_api::time::MediaDuration;
use youcraft_i18n::Lang;

use crate::controls::{Controls, Glance};
use crate::icons::{Glyph, Icon};
use crate::lang::{PlayerLanguage, count, tr};
use crate::menu::Settings;
use crate::state::{Notice, Player};

/// Milliseconds within which a second tap makes a double tap.
const DOUBLE_TAP: f64 = 300.0;

/// The player of `video`, which lasts `duration`, written in `lang`; `theatre` is the page's
/// theatre mode, which the player toggles; `started` runs when the video first starts playing.
#[component]
pub fn VideoPlayer(
    video: VideoId,
    duration: MediaDuration,
    poster: String,
    lang: Lang,
    theatre: RwSignal<bool>,
    #[prop(optional)] started: Option<Callback<()>>,
) -> impl IntoView {
    provide_context(PlayerLanguage(lang));
    let seconds = if duration.timescale == 0 {
        0.0
    } else {
        duration.ticks as f64 / f64::from(duration.timescale)
    };
    let player = Player::new(seconds, theatre);
    let storyboard = RwSignal::new(None::<Storyboard>);
    let page_dir = if youcraft_i18n::rtl(lang) {
        "rtl"
    } else {
        "ltr"
    };
    #[cfg(feature = "hydrate")]
    start(player, video, storyboard);
    #[cfg(not(feature = "hydrate"))]
    let _ = video;
    let last_tap = StoredValue::new(None::<(f64, Side)>);
    // Whether the last press on the surface was a mouse's: touch double taps seek, and the
    // double click browsers make of them must not fill the screen too.
    let by_mouse = StoredValue::new(false);
    let on_surface = move |event: ev::PointerEvent| {
        by_mouse.set_value(event.pointer_type() == "mouse");
        if player.menu.get_untracked().is_some() {
            player.menu.set(None);
            return;
        }
        if event.pointer_type() == "mouse" {
            if event.button() == 0 {
                player.toggle_play();
                player.poke();
            }
            return;
        }
        let side = side_of(&event);
        let now = event.time_stamp();
        let double = last_tap.get_value().is_some_and(|(then, last)| {
            now - then < DOUBLE_TAP && last == side && side != Side::Middle
        });
        last_tap.set_value(Some((now, side)));
        match (double, side) {
            (true, Side::Start) => player.jump(-10.0),
            (true, Side::End) => player.jump(10.0),
            _ if player.active.get_untracked() && player.playing.get_untracked() => {
                player.active.set(false)
            }
            _ => player.poke(),
        }
    };
    view! {
        <div
            class="player"
            node_ref=player.root
            dir="ltr"
            class:active=move || player.active.get() || !player.playing.get()
            class:unstarted=move || !player.started.get()
            class:full-screen=move || player.full_screen.get()
            on:pointermove=move |event: ev::PointerEvent| {
                if event.pointer_type() == "mouse" {
                    player.poke();
                }
            }
            on:pointerleave=move |event: ev::PointerEvent| {
                if event.pointer_type() == "mouse" && player.playing.get_untracked() && player.menu.get_untracked().is_none() {
                    player.active.set(false);
                }
            }
            on:fullscreenchange=move |_| player.full_screen.set(is_full_screen())
        >
            <video
                class="player-video"
                node_ref=player.video
                poster=poster
                preload="metadata"
                playsinline=true
                on:play=move |_| {
                    if !player.started.get_untracked()
                        && let Some(started) = started
                    {
                        started.run(());
                    }
                    player.started.set(true);
                    player.sync();
                    player.poke();
                }
                on:pause=move |_| player.sync()
                on:ended=move |_| player.sync()
                on:timeupdate=move |_| player.sync()
                on:progress=move |_| player.sync()
                on:durationchange=move |_| player.sync()
                on:volumechange=move |_| player.sync()
                on:ratechange=move |_| player.sync()
                on:waiting=move |_| player.waiting.set(true)
                on:playing=move |_| player.waiting.set(false)
                on:canplay=move |_| player.waiting.set(false)
            ></video>
            <div
                class="player-surface"
                on:pointerup=on_surface
                on:dblclick=move |_| {
                    if by_mouse.get_value() {
                        player.toggle_full_screen();
                    }
                }
            ></div>
            <Show when=move || player.waiting.get() && player.playing.get()>
                <div class="player-spinner" aria-hidden="true"></div>
            </Show>
            {move || player.notice.get().map(|(_, notice)| flash(notice))}
            <button
                class="player-center"
                type="button"
                aria-label=move || if player.playing.get() { tr("Pause") } else { tr("Play") }
                on:click=move |_| player.toggle_play()
            >
                {move || {
                    let icon = match (player.playing.get(), player.ended.get()) {
                        (true, _) => Icon::Pause,
                        (false, true) => Icon::Replay,
                        (false, false) => Icon::Play,
                    };
                    view! { <Glyph icon/> }
                }}
            </button>
            <Show when=move || player.unplayable.get()>
                <p class="player-unplayable" dir=page_dir>{tr("This video can't be played in this browser.")}</p>
            </Show>
            <div class="player-scrim" aria-hidden="true"></div>
            <Settings player page_dir/>
            <Controls player glance=Glance { video, storyboard }/>
        </div>
    }
    .into_any()
}

/// Which third of the player a tap fell in; only the browser tells the thirds apart.
#[cfg_attr(not(feature = "hydrate"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Start,
    Middle,
    End,
}

fn side_of(event: &ev::PointerEvent) -> Side {
    #[cfg(feature = "hydrate")]
    {
        use wasm_bindgen::JsCast;
        let width = event
            .current_target()
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
            .map(|element| element.get_bounding_client_rect());
        if let Some(rect) = width
            && rect.width() > 0.0
        {
            let x = (f64::from(event.client_x()) - rect.left()) / rect.width();
            return if x < 1.0 / 3.0 {
                Side::Start
            } else if x > 2.0 / 3.0 {
                Side::End
            } else {
                Side::Middle
            };
        }
    }
    let _ = event;
    Side::Middle
}

fn is_full_screen() -> bool {
    #[cfg(feature = "hydrate")]
    {
        crate::web::window()
            .ok()
            .and_then(|window| window.document())
            .is_some_and(|document| document.fullscreen_element().is_some())
    }
    #[cfg(not(feature = "hydrate"))]
    false
}

/// What flashes over the picture for `notice`.
fn flash(notice: Notice) -> impl IntoView {
    let lang = crate::lang::current();
    match notice {
        Notice::Play => view! { <div class="player-bezel"><Glyph icon=Icon::Play/></div> }.into_any(),
        Notice::Pause => view! { <div class="player-bezel"><Glyph icon=Icon::Pause/></div> }.into_any(),
        Notice::Back(seconds) => view! {
            <div class="player-jump start"><Glyph icon=Icon::Rewind/><span>{count(u64::from(seconds), "{n} second", "{n} seconds")}</span></div>
        }
        .into_any(),
        Notice::Ahead(seconds) => view! {
            <div class="player-jump end"><Glyph icon=Icon::Forward/><span>{count(u64::from(seconds), "{n} second", "{n} seconds")}</span></div>
        }
        .into_any(),
        Notice::Volume(points) => {
            let text = youcraft_i18n::percent(lang, f64::from(points) / 100.0);
            let icon = match points {
                0 => Icon::Muted,
                1..=50 => Icon::VolumeLow,
                _ => Icon::VolumeHigh,
            };
            view! { <div class="player-tag"><Glyph icon/><span>{text}</span></div> }.into_any()
        }
        Notice::Speed(hundredths) => {
            let text = crate::menu::speed_label(hundredths);
            view! { <div class="player-tag"><span>{text}</span></div> }.into_any()
        }
    }
}

/// Start the engine, the storyboard and the keyboard once the page is live; stop them as it
/// leaves.
#[cfg(feature = "hydrate")]
fn start(player: Player, video: VideoId, storyboard: RwSignal<Option<Storyboard>>) {
    use wasm_bindgen::JsCast;
    use youcraft_api::routes;

    Effect::new(move |_| {
        player.start(routes::playlist_of_renditions(video));
        wasm_bindgen_futures::spawn_local(async move {
            let text = crate::web::fetch_text(&routes::storyboard(video)).await;
            if let Some(found) = text.ok().and_then(|text| serde_json::from_str(&text).ok()) {
                storyboard.set(Some(found));
            }
        });
    });
    let keys = window_event_listener(ev::keydown, move |event| {
        let target = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok());
        let tag = target
            .as_ref()
            .map(|element| element.tag_name())
            .unwrap_or_default();
        let editable = target
            .as_ref()
            .and_then(|element| element.get_attribute("contenteditable"))
            .is_some();
        if editable || matches!(tag.as_str(), "INPUT" | "TEXTAREA" | "SELECT") {
            return;
        }
        let key = event.key();
        if matches!(tag.as_str(), "BUTTON" | "A") && matches!(key.as_str(), " " | "Enter") {
            return;
        }
        if key == "Escape" && player.menu.get_untracked().is_some() {
            player.menu.set(None);
            return;
        }
        let modified = event.ctrl_key() || event.alt_key() || event.meta_key();
        if let Some(action) = crate::keys::action(&key, modified) {
            event.prevent_default();
            player.act(action);
        }
    });
    on_cleanup(move || {
        keys.remove();
        player.stop();
    });
}
