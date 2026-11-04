//! What an avatar shows of a channel: its photo, at the size the avatar is drawn, or else the
//! first letter of its name on the avatar's colour.

use leptos::prelude::*;
use youcraft_api::records::ChannelCard;
use youcraft_api::routes::{self, PHOTO_SIZES};

/// How large an avatar is drawn, which picks the size of its photo: twice its CSS size, for
/// screens of two pixels to the CSS pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    /// The guide's and replies' (24 CSS pixels).
    Small,
    /// The masthead's, the cards' and the comments' (36 to 40).
    Regular,
    /// The settings' and the studio's (64 to 88).
    Large,
    /// The channel page's and the search results' (136 to 160).
    Huge,
}

impl Size {
    /// The size of the photo drawn at this size, one of [`PHOTO_SIZES`].
    fn pixels(self) -> u32 {
        let index = match self {
            Size::Small => 0,
            Size::Regular => 1,
            Size::Large => 2,
            Size::Huge => 3,
        };
        PHOTO_SIZES.get(index).copied().unwrap_or(320)
    }
}

/// The first letter of `name`, as its avatar shows it.
pub fn initial(name: &str) -> String {
    name.chars().next().map(String::from).unwrap_or_default()
}

/// The inside of an avatar of `channel` drawn at `size`.
#[component]
pub fn Face(channel: ChannelCard, size: Size) -> impl IntoView {
    match channel.photo {
        Some(version) => {
            let src = routes::photo(channel.id, &version, size.pixels());
            view! { <img src=src alt="" loading="lazy" decoding="async"/> }.into_any()
        }
        None => initial(&channel.name).into_any(),
    }
}
