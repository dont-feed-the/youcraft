//! A video's thumbnail and its duration, leading to its watch page. As a mouse moves across it,
//! it shows the video's frame under the pointer and a bar as far along; a touch shows nothing.
//!
//! The frames come from the video's skim sheet ([`skim`]), asked for when a mouse first enters,
//! and nothing covers the thumbnail until it has arrived, nor ever for a video without one. The
//! sheet is one image as large as [`skim::COLUMNS`] × [`skim::ROWS`] thumbnails, moved so that
//! the tile under the pointer fills the thumbnail. The left edge is the start of the video in
//! every language, as on the player's seek bar. Sizes and offsets reach the elements by script,
//! which the page's content security policy allows where it forbids `style` attributes.

use leptos::ev;
use leptos::prelude::*;
use youcraft_api::records::VideoCard;
use youcraft_api::{routes, skim};

/// How far the skim sheet of a thumbnail has come.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sheet {
    /// No mouse has entered yet.
    Unasked,
    /// Asked for, on its way.
    Coming,
    /// Arrived.
    Arrived,
    /// The video has none.
    Missing,
}

/// The `srcset` of the thumbnail of `card`: every width it is made at.
fn widths(card: &VideoCard) -> String {
    routes::THUMBNAIL_WIDTHS
        .iter()
        .map(|width| format!("{} {width}w", card.thumbnail_at(*width)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Where along its element the mouse of `event` is, from 0 (left) to 1 (right); `None` for a
/// touch or a pen, which do not skim.
fn mouse_along(event: &ev::PointerEvent) -> Option<f64> {
    #[cfg(feature = "hydrate")]
    {
        use wasm_bindgen::JsCast;
        if event.pointer_type() != "mouse" {
            return None;
        }
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

/// `part` of `whole` as a CSS percentage.
fn percent(part: u32, whole: u32) -> String {
    format!("{}%", 100.0 * f64::from(part) / f64::from(whole.max(1)))
}

/// The thumbnail of the video of `card`: its `width`-pixel image by default, the one `sizes`
/// chooses where the browser decides.
#[component]
pub fn VideoThumbnail(card: VideoCard, width: u32, sizes: &'static str) -> impl IntoView {
    let video = card.id;
    let pointer = RwSignal::new(None::<f64>);
    let sheet = RwSignal::new(Sheet::Unasked);
    let follow = move |event: ev::PointerEvent| {
        let Some(fraction) = mouse_along(&event) else {
            return;
        };
        if sheet.get_untracked() == Sheet::Unasked {
            sheet.set(Sheet::Coming);
        }
        pointer.set(Some(fraction));
    };
    // The sheet stays in the page once asked for, so that it comes only once.
    let asked = Memo::new(move |_| matches!(sheet.get(), Sheet::Coming | Sheet::Arrived));
    let skim = move || {
        asked.get().then(|| {
            let offset = move || {
                let (column, row) = skim::place(skim::tile_under(pointer.get().unwrap_or(0.0)));
                format!(
                    "-{} -{}",
                    percent(column, skim::COLUMNS),
                    percent(row, skim::ROWS)
                )
            };
            view! {
                <div class="skim" dir="ltr" class:shown=move || sheet.get() == Sheet::Arrived && pointer.get().is_some()>
                    <img
                        class="skim-sheet"
                        src=routes::skim(video)
                        alt=""
                        decoding="async"
                        style:inline-size=percent(skim::COLUMNS, 1)
                        style:block-size=percent(skim::ROWS, 1)
                        style:translate=offset
                        on:load=move |_| sheet.set(Sheet::Arrived)
                        on:error=move |_| sheet.set(Sheet::Missing)
                    />
                    <div class="skim-bar" style:inline-size=move || format!("{}%", 100.0 * pointer.get().unwrap_or(0.0))></div>
                </div>
            }
        })
    };
    view! {
        <a
            class="thumbnail"
            href=routes::watch(video)
            tabindex="-1"
            aria-hidden="true"
            on:pointerenter=follow
            on:pointermove=follow
            on:pointerleave=move |_| pointer.set(None)
        >
            <img src=card.thumbnail_at(width) srcset=widths(&card) sizes=sizes
                alt="" loading="lazy" decoding="async"/>
            {skim}
            <span class="duration">{youcraft_i18n::when::clock(card.duration.whole_seconds())}</span>
        </a>
    }
    .into_any()
}
