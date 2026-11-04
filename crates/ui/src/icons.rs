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
    /// An arrow up out of a tray: upload.
    Upload,
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
    /// Three dots in a column: more.
    More,
    /// A toothed wheel: settings.
    Settings,
    /// An arrow leaving a door: sign out.
    SignOut,
    /// A clapperboard: the studio.
    Studio,
    /// A circle, half of it shaded: appearance.
    Appearance,
    /// A character and a letter: language.
    Translate,
    /// A tick: the chosen entry.
    Check,
    /// An arrow pointing back along the line: back.
    Back,
    /// A chevron pointing along the line: more inside.
    Forward,
    /// A key: personal tokens.
    Key,
    /// A shield with a tick: the site's administration.
    Shield,
    /// An envelope: invitations.
    Mail,
    /// Lines and a plus: save to a list.
    Save,
    /// Lines growing shorter: sort.
    Sort,
    /// A pencil: edit.
    Edit,
    /// A bin: delete.
    Delete,
    /// A circle struck through: block.
    Block,
    /// A cross: close.
    Close,
    /// A chevron pointing down: show more below.
    Expand,
    /// A plus: add.
    Plus,
    /// A padlock: private.
    Lock,
    /// A speech bubble: a conversation.
    Chat,
    /// Angle brackets around a slash: source code.
    Code,
}

impl Icon {
    /// Whether the icon points along the line of text, and so mirrors right to left. Clocks,
    /// thumbs and media keep their direction.
    fn directional(self) -> bool {
        matches!(
            self,
            Icon::Share | Icon::SignOut | Icon::Back | Icon::Forward | Icon::Chat
        )
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
            Icon::Upload => &["M12 15V4", "M7.5 8.5L12 4l4.5 4.5", "M4 15v5h16v-5"],
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
            Icon::More => &[
                "M12 4a1 1 0 1 0 0 2a1 1 0 1 0 0-2",
                "M12 11a1 1 0 1 0 0 2a1 1 0 1 0 0-2",
                "M12 18a1 1 0 1 0 0 2a1 1 0 1 0 0-2",
            ],
            Icon::Settings => &[
                "M12 9a3 3 0 1 0 0 6a3 3 0 1 0 0-6",
                "M12 2.5v3",
                "M12 18.5v3",
                "M2.5 12h3",
                "M18.5 12h3",
                "M5.3 5.3l2.1 2.1",
                "M16.6 16.6l2.1 2.1",
                "M5.3 18.7l2.1-2.1",
                "M16.6 7.4l2.1-2.1",
            ],
            Icon::SignOut => &["M10 4H5v16h5", "M14 8l4 4-4 4", "M18 12H9"],
            Icon::Studio => &[
                "M3 9h18v11H3z",
                "M3 9l2-5 16 0-2 5",
                "M9 4l-2 5",
                "M15 4l-2 5",
            ],
            Icon::Appearance => &[
                "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18",
                "M12 3v18",
                "M12 8h5",
                "M12 12h7",
                "M12 16h5",
            ],
            Icon::Translate => &[
                "M3 5h10",
                "M8 3v2",
                "M11 5c-1 4-3.5 7-7 9",
                "M6 9c1.5 2.5 3.5 4 6 5",
                "M13 21l4-9 4 9",
                "M14.5 18h5",
            ],
            Icon::Check => &["M5 12.5l4.5 4.5L19 7"],
            Icon::Back => &["M20 12H4", "M10 6l-6 6 6 6"],
            Icon::Forward => &["M9 5l7 7-7 7"],
            Icon::Shield => &[
                "M12 3l7 3v5c0 4.5-3 8.5-7 10c-4-1.5-7-5.5-7-10V6z",
                "M9 12l2 2l4-4",
            ],
            Icon::Key => &[
                "M8 9a4 4 0 1 0 0 6a4 4 0 1 0 0-6",
                "M11.5 12H21",
                "M18 12v3",
                "M15 12v2",
            ],
            Icon::Mail => &["M3 6h18v12H3z", "M3 6l9 7 9-7"],
            Icon::Save => &["M3 6h13", "M3 11h13", "M3 16h8", "M18 13v8", "M14 17h8"],
            Icon::Sort => &["M3 6h18", "M3 12h12", "M3 18h6"],
            Icon::Edit => &[
                "M4 20l1-4.5L15.5 5a2.1 2.1 0 0 1 3 3L8 18.5z",
                "M13.5 7l3 3",
            ],
            Icon::Delete => &[
                "M4 6h16",
                "M9 6V4h6v2",
                "M6 6l1 14h10l1-14",
                "M10 10v6",
                "M14 10v6",
            ],
            Icon::Block => &["M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18", "M5.6 5.6l12.8 12.8"],
            Icon::Close => &["M6 6l12 12", "M18 6L6 18"],
            Icon::Expand => &["M6 9l6 6 6-6"],
            Icon::Plus => &["M12 5v14", "M5 12h14"],
            Icon::Lock => &["M6 11h12v9H6z", "M8.5 11V8a3.5 3.5 0 0 1 7 0v3"],
            Icon::Chat => &["M4 5h16v11H9l-5 4z", "M8 9h8", "M8 12h5"],
            Icon::Code => &["M8 7l-5 5 5 5", "M16 7l5 5-5 5", "M14 4l-4 16"],
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

/// YouCraft's mark: a rocket flying forward as a play button points, in the mark's own colours.
#[component]
pub fn Mark() -> impl IntoView {
    view! {
        <svg class="mark" viewBox="0 0 32 24" aria-hidden="true" focusable="false">
            <path class="flame" d="M11.5 9.4C8 9.4 6.5 11 3.5 12C6.5 13 8 14.6 11.5 14.6z"/>
            <path class="fins" d="M16.5 8.2L11 4.5L9 4.5L11.5 8.2zM16.5 15.8L11 19.5L9 19.5L11.5 15.8z"/>
            <path class="body" d="M28.5 12C25.5 15.2 21 16 17 16L11 16L11 8L17 8C21 8 25.5 8.8 28.5 12z"/>
            <circle class="window" cx="19" cy="12" r="2.3"/>
        </svg>
    }
}
