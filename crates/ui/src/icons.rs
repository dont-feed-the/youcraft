//! YouCraft's own icons: simple strokes on a 24-unit grid, drawn in the current text colour.

use leptos::prelude::*;

/// Which icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// Three lines: open or close the guide.
    Menu,
    /// A lens: search.
    Search,
    /// A plus in a circle: create.
    Create,
    /// A head and shoulders: an account.
    Account,
    /// A house: home.
    Home,
    /// Two stacked frames with a triangle: subscriptions.
    Subscriptions,
    /// A frame with a corner fold: everything of yours.
    You,
    /// A clock turning back: history.
    History,
    /// Lines and a triangle: playlists.
    Playlists,
    /// A clock: watch later.
    WatchLater,
    /// A raised thumb: liked.
    Like,
    /// A lowered thumb: disliked.
    Dislike,
    /// An arrow leaving a curve: share.
    Share,
}

impl Icon {
    /// Whether the icon points along the line of text, and so mirrors right to left. Clocks,
    /// thumbs and media keep their direction.
    fn directional(self) -> bool {
        matches!(self, Icon::Share)
    }

    /// The stroke paths of the icon.
    fn paths(self) -> &'static [&'static str] {
        match self {
            Icon::Menu => &["M3 6h18", "M3 12h18", "M3 18h18"],
            Icon::Search => &[
                "M10.5 4a6.5 6.5 0 1 0 0 13a6.5 6.5 0 1 0 0-13",
                "M15.3 15.3L21 21",
            ],
            Icon::Create => &["M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18", "M12 8v8", "M8 12h8"],
            Icon::Account => &[
                "M12 4a4 4 0 1 0 0 8a4 4 0 1 0 0-8",
                "M4 21c0-4.4 3.6-7 8-7s8 2.6 8 7",
            ],
            Icon::Home => &["M3 11L12 4l9 7", "M5 9.5V20h5v-6h4v6h5V9.5"],
            Icon::Subscriptions => &[
                "M6 4h12",
                "M4 7.5h16",
                "M3 11h18v10H3z",
                "M10 13.5v5l4.3-2.5z",
            ],
            Icon::You => &["M4 4h11l5 5v11H4z", "M15 4v5h5", "M10 11v6l5-3z"],
            Icon::History => &["M4.5 12a7.5 7.5 0 1 0 2.2-5.3", "M4 4v4h4", "M12 8v4.5l3 2"],
            Icon::Playlists => &["M3 6h13", "M3 11h13", "M3 16h8", "M15 14v6l5-3z"],
            Icon::WatchLater => &["M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18", "M12 7v5l3.5 2"],
            Icon::Like => &[
                "M7 10v10H3V10z",
                "M7 10l4-7c1.7 0 2.5 1.2 2 3l-1 3.5h6.5c1.2 0 2 1 1.8 2.2l-1.4 6.6c-.2 1-1 1.7-2 1.7H7",
            ],
            Icon::Dislike => &[
                "M17 14V4h4v10z",
                "M17 14l-4 7c-1.7 0-2.5-1.2-2-3l1-3.5H5.5c-1.2 0-2-1-1.8-2.2l1.4-6.6c.2-1 1-1.7 2-1.7H17",
            ],
            Icon::Share => &["M14 5l7 6.5-7 6.5", "M21 11.5h-8C7.5 11.5 4 14.5 3 20"],
        }
    }
}

/// An icon, 24 by 24 pixels, hidden from screen readers: its button or link carries the label.
#[component]
pub fn Glyph(icon: Icon) -> impl IntoView {
    view! {
        <svg class="glyph" class:directional=icon.directional() viewBox="0 0 24 24" aria-hidden="true" focusable="false">
            {icon.paths().iter().map(|d| view! { <path d=*d/> }).collect_view()}
        </svg>
    }
}

/// YouCraft's mark: a folded paper triangle, its two facets in the accent colour.
#[component]
pub fn Mark() -> impl IntoView {
    view! {
        <svg class="mark" viewBox="0 0 32 24" aria-hidden="true" focusable="false">
            <path class="facet-light" d="M6 2L28 12L6 12z"/>
            <path class="facet-dark" d="M6 12L28 12L6 22z"/>
            <path class="fold" d="M6 12L28 12"/>
        </svg>
    }
}
