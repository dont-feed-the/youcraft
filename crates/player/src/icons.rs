//! The player's own icons, drawn on a 24-unit grid in the current colour: solid shapes for
//! playing and pausing, strokes for the rest.

use leptos::prelude::*;

/// Which icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// A triangle pointing forward in time.
    Play,
    /// Two bars.
    Pause,
    /// An arrow turning back to the start.
    Replay,
    /// A speaker sending two waves.
    VolumeHigh,
    /// A speaker sending one wave.
    VolumeLow,
    /// A speaker, crossed.
    Muted,
    /// A wheel with eight teeth.
    Settings,
    /// A wide frame: theatre mode.
    Theatre,
    /// A frame within the frame: the default view.
    DefaultView,
    /// Four corners pointing out.
    FullScreen,
    /// Four corners pointing in.
    ExitFullScreen,
    /// Two triangles back.
    Rewind,
    /// Two triangles forward.
    Forward,
    /// A chevron back along the line of text.
    Back,
    /// A chevron forward along the line of text.
    Next,
    /// A tick.
    Check,
}

impl Icon {
    /// Whether it points along the line of text, and so mirrors right to left. Media icons point
    /// along time, which runs left to right everywhere.
    fn directional(self) -> bool {
        matches!(self, Icon::Back | Icon::Next)
    }

    /// Whether it is a solid shape rather than strokes.
    fn solid(self) -> bool {
        matches!(
            self,
            Icon::Play | Icon::Pause | Icon::Rewind | Icon::Forward
        )
    }

    fn paths(self) -> &'static [&'static str] {
        match self {
            Icon::Play => {
                &["M8 5.5v13a1 1 0 0 0 1.5.9l10-6.5a1 1 0 0 0 0-1.8l-10-6.5A1 1 0 0 0 8 5.5z"]
            }
            Icon::Pause => &["M6 5h4v14H6z", "M14 5h4v14h-4z"],
            Icon::Replay => &["M5 12a7 7 0 1 0 2-4.9", "M4.5 3.5v4h4"],
            Icon::VolumeHigh => &[
                "M4 9.5h3l4.5-4v13L7 14.5H4z",
                "M15 9a4 4 0 0 1 0 6",
                "M17.5 6.5a7.5 7.5 0 0 1 0 11",
            ],
            Icon::VolumeLow => &["M4 9.5h3l4.5-4v13L7 14.5H4z", "M15 9a4 4 0 0 1 0 6"],
            Icon::Muted => &[
                "M4 9.5h3l4.5-4v13L7 14.5H4z",
                "M15.5 9.5l5 5",
                "M20.5 9.5l-5 5",
            ],
            Icon::Settings => &[
                "M12 9a3 3 0 1 0 0 6a3 3 0 1 0 0-6",
                "M12 5.5a6.5 6.5 0 1 0 0 13a6.5 6.5 0 1 0 0-13",
                "M12 2.5v3",
                "M12 18.5v3",
                "M2.5 12h3",
                "M18.5 12h3",
                "M5.3 5.3l2.1 2.1",
                "M16.6 16.6l2.1 2.1",
                "M5.3 18.7l2.1-2.1",
                "M16.6 7.4l2.1-2.1",
            ],
            Icon::Theatre => &["M2.5 6.5h19v11h-19z"],
            Icon::DefaultView => &["M2.5 6.5h19v11h-19z", "M6.5 9.5h11v5h-11z"],
            Icon::FullScreen => &["M4 9V4h5", "M15 4h5v5", "M20 15v5h-5", "M9 20H4v-5"],
            Icon::ExitFullScreen => &["M9 4v5H4", "M20 9h-5V4", "M15 20v-5h5", "M4 15h5v5"],
            Icon::Rewind => &["M11 6v12l-8-6z", "M20 6v12l-8-6z"],
            Icon::Forward => &["M4 6v12l8-6z", "M13 6v12l8-6z"],
            Icon::Back => &["M15 5l-7 7 7 7"],
            Icon::Next => &["M9 5l7 7-7 7"],
            Icon::Check => &["M4.5 12.5l5 5 10-11"],
        }
    }
}

/// An icon, hidden from screen readers: its button carries the label.
#[component]
pub fn Glyph(icon: Icon) -> impl IntoView {
    let class = match (icon.solid(), icon.directional()) {
        (true, _) => "player-glyph solid",
        (false, true) => "player-glyph directional",
        (false, false) => "player-glyph",
    };
    view! {
        <svg class=class viewBox="0 0 24 24" aria-hidden="true">
            {icon.paths().iter().map(|d| view! { <path d=*d/> }).collect_view()}
        </svg>
    }
}
