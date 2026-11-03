//! The bar under the picture: the seek bar with its preview, then play, volume and time at the
//! start, settings, theatre mode and full screen at the end.
//!
//! Sliders are dragged with any pointer, captured so that a drag may leave them, and read the
//! pointer's position along them left to right, as the bar is laid out in every language. Their
//! levels reach the stylesheet as custom properties set by script, which the page's content
//! security policy allows where it forbids `style` attributes in the markup.

use leptos::ev;
use leptos::html::Div;
use leptos::prelude::*;
use youcraft_api::records::Storyboard;
use youcraft_api::{VideoId, routes};
use youcraft_i18n::when::clock;

use crate::icons::{Glyph, Icon};
use crate::lang::tr;
use crate::state::{Menu, Player};

/// What the seek bar previews under the pointer: the storyboard of the video, once loaded.
#[derive(Clone, Copy)]
pub struct Glance {
    /// The video.
    pub video: VideoId,
    /// Its storyboard, if it has one.
    pub storyboard: RwSignal<Option<Storyboard>>,
}

/// Where along the element the pointer of `event` is, from 0 (left) to 1 (right).
fn along(event: &ev::PointerEvent) -> Option<f64> {
    #[cfg(feature = "hydrate")]
    {
        use wasm_bindgen::JsCast;
        let element: web_sys::Element = event.current_target()?.dyn_into().ok()?;
        let rect = element.get_bounding_client_rect();
        if rect.width() <= 0.0 {
            return None;
        }
        Some(((f64::from(event.client_x()) - rect.left()) / rect.width()).clamp(0.0, 1.0))
    }
    #[cfg(not(feature = "hydrate"))]
    {
        let _ = event;
        None
    }
}

/// Keep the pointer of `event` on its element until it is released.
fn capture(event: &ev::PointerEvent) {
    #[cfg(feature = "hydrate")]
    {
        use wasm_bindgen::JsCast;
        if let Some(element) = event
            .current_target()
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
        {
            let _ = element.set_pointer_capture(event.pointer_id());
        }
    }
    let _ = event;
}

/// `part` of `whole`, from 0 to 1, as a CSS number.
fn share(part: f64, whole: f64) -> String {
    let ratio = if whole > 0.0 {
        (part / whole).clamp(0.0, 1.0)
    } else {
        0.0
    };
    ratio.to_string()
}

/// Keep the custom property `name` of `node` at `value`, set by script once the page is live.
fn bind(node: NodeRef<Div>, name: &'static str, value: impl Fn() -> String + 'static) {
    #[cfg(feature = "hydrate")]
    Effect::new(move |_| {
        let value = value();
        if let Some(element) = node.get() {
            let _ = web_sys::HtmlElement::style(&element).set_property(name, &value);
        }
    });
    #[cfg(not(feature = "hydrate"))]
    let _ = (node, name, value);
}

/// The controls of `player`.
#[component]
pub fn Controls(player: Player, glance: Glance) -> impl IntoView {
    let time_text = move || {
        format!(
            "{} / {}",
            clock(player.time.get().max(0.0) as u64),
            clock(player.duration.get().max(0.0) as u64)
        )
    };
    view! {
        <div class="player-chrome">
            <SeekBar player glance/>
            <div class="player-controls">
                <div class="player-group">
                    <button
                        class="player-button player-play"
                        type="button"
                        data-tip=move || if player.playing.get() { tr("Pause") } else { tr("Play") }
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
                    <Volume player/>
                    <span class="player-time">{time_text}</span>
                </div>
                <div class="player-group end">
                    <button
                        class="player-button"
                        type="button"
                        data-tip=tr("Settings")
                        aria-label=tr("Settings")
                        aria-expanded=move || player.menu.get().is_some().to_string()
                        on:click=move |_| player.menu.update(|menu| {
                            *menu = if menu.is_some() { None } else { Some(Menu::Main) };
                        })
                    >
                        <Glyph icon=Icon::Settings/>
                    </button>
                    <button
                        class="player-button player-theatre"
                        type="button"
                        data-tip=move || if player.theatre.get() { tr("Default view") } else { tr("Theatre mode") }
                        aria-label=move || if player.theatre.get() { tr("Default view") } else { tr("Theatre mode") }
                        on:click=move |_| player.theatre.update(|on| *on = !*on)
                    >
                        {move || view! { <Glyph icon=if player.theatre.get() { Icon::DefaultView } else { Icon::Theatre }/> }}
                    </button>
                    <button
                        class="player-button"
                        type="button"
                        data-tip=move || if player.full_screen.get() { tr("Exit full screen") } else { tr("Full screen") }
                        aria-label=move || if player.full_screen.get() { tr("Exit full screen") } else { tr("Full screen") }
                        on:click=move |_| player.toggle_full_screen()
                    >
                        {move || view! { <Glyph icon=if player.full_screen.get() { Icon::ExitFullScreen } else { Icon::FullScreen }/> }}
                    </button>
                </div>
            </div>
        </div>
    }
    .into_any()
}

/// The mute button, and the volume slider that opens beside it.
#[component]
fn Volume(player: Player) -> impl IntoView {
    let dragging = RwSignal::new(false);
    let shown = move || {
        if player.muted.get() {
            0
        } else {
            player.volume.get()
        }
    };
    let set = move |event: &ev::PointerEvent| {
        if let Some(fraction) = along(event) {
            player.set_volume((fraction * 100.0).round() as u32);
        }
    };
    let slider = NodeRef::<Div>::new();
    bind(slider, "--level", move || share(f64::from(shown()), 100.0));
    view! {
        <div class="player-volume" class:dragging=move || dragging.get()>
            <button
                class="player-button"
                type="button"
                data-tip=move || if shown() == 0 { tr("Unmute") } else { tr("Mute") }
                aria-label=move || if shown() == 0 { tr("Unmute") } else { tr("Mute") }
                on:click=move |_| player.toggle_mute()
            >
                {move || {
                    let icon = match shown() {
                        0 => Icon::Muted,
                        1..=50 => Icon::VolumeLow,
                        _ => Icon::VolumeHigh,
                    };
                    view! { <Glyph icon/> }
                }}
            </button>
            <div
                class="player-slider"
                node_ref=slider
                role="slider"
                tabindex="0"
                aria-label=tr("Volume")
                aria-valuemin="0"
                aria-valuemax="100"
                aria-valuenow=move || shown().to_string()
                on:pointerdown=move |event: ev::PointerEvent| {
                    dragging.set(true);
                    capture(&event);
                    set(&event);
                }
                on:pointermove=move |event: ev::PointerEvent| {
                    if dragging.get_untracked() {
                        set(&event);
                    }
                }
                on:pointerup=move |_| dragging.set(false)
                on:pointercancel=move |_| dragging.set(false)
            >
                <div class="player-slider-fill"></div>
                <div class="player-slider-thumb"></div>
            </div>
        </div>
    }
    .into_any()
}

/// The seek bar: what has played, what is buffered, and a preview of the time under the pointer.
#[component]
fn SeekBar(player: Player, glance: Glance) -> impl IntoView {
    let hover = RwSignal::new(None::<f64>);
    let scrubbing = RwSignal::new(false);
    let bar = NodeRef::<Div>::new();
    bind(bar, "--played", move || {
        share(player.time.get(), player.duration.get())
    });
    bind(bar, "--buffered", move || {
        share(player.buffered.get(), player.duration.get())
    });
    bind(bar, "--hovered", move || {
        share(hover.get().unwrap_or(0.0), 1.0)
    });
    let at = move |event: &ev::PointerEvent| {
        let fraction = along(event);
        hover.set(fraction);
        if scrubbing.get_untracked()
            && let Some(fraction) = fraction
        {
            player.seek_fraction(fraction);
        }
    };
    let preview = move || {
        let fraction = hover.get()?;
        let duration = player.duration.get();
        let seconds = fraction * duration;
        let tile = glance
            .storyboard
            .get()
            .and_then(|board| tile(glance.video, &board, seconds, duration));
        let left = format!(
            "clamp(var(--preview-half), {}%, calc(100% - var(--preview-half)))",
            fraction * 100.0
        );
        Some(view! {
            <div class="seek-preview" style:inset-inline-start=left>
                {tile.map(|(sheet, x, y, width, height)| view! {
                    <div
                        class="seek-frame"
                        style:background-image=format!("url(\"{sheet}\")")
                        style:background-position=format!("-{x}px -{y}px")
                        style:inline-size=format!("{width}px")
                        style:block-size=format!("{height}px")
                    ></div>
                })}
                <span class="seek-time">{clock(seconds.max(0.0) as u64)}</span>
            </div>
        })
    };
    view! {
        <div
            class="seek"
            node_ref=bar
            class:scrubbing=move || scrubbing.get()
            role="slider"
            tabindex="0"
            aria-label=tr("Seek")
            aria-valuemin="0"
            aria-valuemax=move || (player.duration.get().max(0.0) as u64).to_string()
            aria-valuenow=move || (player.time.get().max(0.0) as u64).to_string()
            aria-valuetext=move || clock(player.time.get().max(0.0) as u64)
            on:pointerdown=move |event: ev::PointerEvent| {
                scrubbing.set(true);
                capture(&event);
                at(&event);
                if let Some(fraction) = hover.get_untracked() {
                    player.seek_fraction(fraction);
                }
            }
            on:pointermove=move |event: ev::PointerEvent| at(&event)
            on:pointerup=move |_| scrubbing.set(false)
            on:pointercancel=move |_| scrubbing.set(false)
            on:pointerleave=move |_| {
                if !scrubbing.get_untracked() {
                    hover.set(None);
                }
            }
        >
            <div class="seek-track">
                <div class="seek-buffered"></div>
                <div class="seek-hover"></div>
                <div class="seek-played"></div>
            </div>
            <div class="seek-thumb"></div>
            {preview}
        </div>
    }
    .into_any()
}

/// The storyboard tile showing `seconds` of a video lasting `duration` seconds: its sheet's
/// address, its offset in the sheet and its size, all in pixels; `None` for a storyboard that is
/// empty or malformed. Tile `i` shows `i × interval`, so the video has `⌈duration / interval⌉`.
pub fn tile(
    video: VideoId,
    board: &Storyboard,
    seconds: f64,
    duration: f64,
) -> Option<(String, u32, u32, u32, u32)> {
    let per_sheet = board.columns.checked_mul(board.rows).filter(|n| *n > 0)?;
    if board.interval.ticks == 0 || board.interval.timescale == 0 {
        return None;
    }
    let tiles_of = |seconds: f64| {
        seconds.max(0.0) * f64::from(board.interval.timescale) / board.interval.ticks as f64
    };
    let in_sheets = u64::from(per_sheet).checked_mul(u64::from(board.sheets))?;
    let in_video = tiles_of(duration).ceil() as u64;
    let last = in_sheets.min(in_video).checked_sub(1)?;
    let index = u32::try_from((tiles_of(seconds) as u64).min(last)).ok()?;
    let sheet = index / per_sheet;
    let within = index % per_sheet;
    let (column, row) = (within % board.columns, within / board.columns);
    Some((
        routes::storyboard_sheet(video, sheet),
        column.checked_mul(board.tile_width)?,
        row.checked_mul(board.tile_height)?,
        board.tile_width,
        board.tile_height,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use youcraft_api::time::MediaDuration;

    #[test]
    fn tiles_are_found_row_by_row_across_sheets() {
        let video = VideoId::from_bits(0);
        let board = Storyboard {
            interval: MediaDuration {
                ticks: 2,
                timescale: 1,
            },
            tile_width: 160,
            tile_height: 90,
            columns: 10,
            rows: 10,
            sheets: 2,
        };
        let (sheet, x, y, ..) = tile(video, &board, 0.0, 400.0).unwrap();
        assert_eq!(
            (sheet.as_str(), x, y),
            ("/media/AAAAAAAAAAA/storyboard/0.jpg", 0, 0)
        );
        // 25 s is tile 12: second row, third column.
        assert_eq!(
            tile(video, &board, 25.0, 400.0).map(|t| (t.1, t.2)),
            Some((320, 90))
        );
        // 200 s is tile 100, the first of the second sheet; past the end stays on the last.
        assert_eq!(
            tile(video, &board, 200.0, 400.0).unwrap().0,
            "/media/AAAAAAAAAAA/storyboard/1.jpg"
        );
        assert_eq!(
            tile(video, &board, 1e9, 1e9).map(|t| (t.1, t.2)),
            Some((1440, 810))
        );
        let empty = Storyboard { sheets: 0, ..board };
        assert_eq!(tile(video, &empty, 1.0, 10.0), None);
        // A video of 24 s has tiles 0 to 11; its end shows the last of them.
        assert_eq!(
            tile(video, &board, 24.0, 24.0).map(|t| (t.1, t.2)),
            Some((160, 90))
        );
        assert_eq!(tile(video, &board, 1.0, 0.0), None);
    }
}
