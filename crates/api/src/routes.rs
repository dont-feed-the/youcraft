//! The URL scheme: YouTube's addresses, and where media and thumbnails live.
//!
//! Pages are found where YouTube keeps them (`/watch?v=…`, `/@handle`, `/results?search_query=…`,
//! `/feed/…`, `/playlist?list=…`), so a YouTube address works once its host is changed. Each
//! function writes one address; the router and the server read the same shapes.

use crate::ids::{ChannelId, VideoId};

/// The home page.
pub const HOME: &str = "/";
/// The page that answers 200 while the server runs.
pub const HEALTH: &str = "/health";
/// Where commands are posted: `/api/v1/<command id>`.
pub const COMMANDS: &str = "/api/v1";
/// The subscriptions feed.
pub const SUBSCRIPTIONS: &str = "/feed/subscriptions";
/// The viewer's history.
pub const HISTORY: &str = "/feed/history";
/// The viewer's playlists.
pub const PLAYLISTS: &str = "/feed/playlists";

/// The watch page of `video`.
pub fn watch(video: VideoId) -> String {
    format!("/watch?v={video}")
}

/// The page of the channel whose handle is `handle`.
pub fn channel(handle: &str) -> String {
    format!("/@{}", encode_component(handle))
}

/// The results page for `query`.
pub fn results(query: &str) -> String {
    format!("/results?search_query={}", encode_component(query))
}

/// The address a command `id` is posted to.
pub fn command(id: &str) -> String {
    format!("{COMMANDS}/{id}")
}

/// The thumbnail of `video`, `width` pixels wide (one of [`THUMBNAIL_WIDTHS`]).
pub fn thumbnail(video: VideoId, width: u32) -> String {
    format!("/media/{video}/thumbnail/{width}.jpg")
}

/// The widths every thumbnail is made at, smallest first.
pub const THUMBNAIL_WIDTHS: [u32; 5] = [168, 246, 336, 480, 720];

/// The HLS playlist that lists the renditions of `video`.
pub fn playlist_of_renditions(video: VideoId) -> String {
    format!("/media/{video}/master.m3u8")
}

/// The avatar of `channel`, `size` pixels square.
pub fn avatar(channel: ChannelId, size: u32) -> String {
    format!("/media/channel/{channel}/avatar/{size}.jpg")
}

/// `text` with every byte outside RFC 3986's unreserved set percent-encoded, as a path segment
/// or query value needs; letters of any script are encoded too, and browsers show them decoded.
pub fn encode_component(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_youtubes() {
        let video = VideoId::from_bits(0);
        assert_eq!(watch(video), "/watch?v=AAAAAAAAAAA");
        assert_eq!(channel("youcraft"), "/@youcraft");
        assert_eq!(
            results("chat noir & blanc"),
            "/results?search_query=chat%20noir%20%26%20blanc"
        );
        assert_eq!(command("feed.home"), "/api/v1/feed.home");
    }

    #[test]
    fn components_encode_every_reserved_byte() {
        assert_eq!(encode_component("a/b?c=d#é"), "a%2Fb%3Fc%3Dd%23%C3%A9");
        assert_eq!(encode_component("A-z_0.9~"), "A-z_0.9~");
    }
}
