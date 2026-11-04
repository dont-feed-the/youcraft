//! YouCraft's video player: the renditions of an HLS master playlist streamed into `<video>`
//! through Media Source Extensions, behind controls laid out and answering as YouTube's do.
//!
//! [`playlist`] reads the playlists into exact times, [`schedule`] decides from what is buffered
//! what to fetch next, [`abr`] chooses a rendition, [`cmaf`] reads how long a frame lasts,
//! [`time`] keeps media time in integers, [`keys`] maps the keyboard; these are plain Rust,
//! tested natively. With the `hydrate` feature the browser side runs: [`engine`] feeds the media
//! source over [`web`]'s calls, and the [`VideoPlayer`] component wires the element, the engine
//! and the controls. The server renders the same component's markup without it.

pub mod abr;
pub mod cmaf;
mod controls;
#[cfg(feature = "hydrate")]
mod engine;
mod icons;
pub mod keys;
mod menu;
mod player;
pub mod playlist;
pub mod schedule;
mod state;
pub mod time;
#[cfg(feature = "hydrate")]
mod web;

pub use player::VideoPlayer;
