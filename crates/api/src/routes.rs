//! The URL scheme: YouTube's addresses, and where media and thumbnails live.
//!
//! Pages are found where YouTube keeps them (`/watch?v=…`, `/@handle`, `/results?search_query=…`,
//! `/feed/…`, `/playlist?list=…`), so a YouTube address works once its host is changed. Each
//! function writes one address; the router and the server read the same shapes.

use crate::ids::{ChannelId, List, VideoId};

/// The home page.
pub const HOME: &str = "/";
/// The page that answers 200 while the server runs.
pub const HEALTH: &str = "/health";
/// Where commands are posted: `/api/v1/<command id>`.
pub const COMMANDS: &str = "/api/v1";
/// Where agents reach the commands as tools, over the Model Context Protocol.
pub const MCP: &str = "/mcp";
/// The subscriptions feed.
pub const SUBSCRIPTIONS: &str = "/feed/subscriptions";
/// The viewer's history.
pub const HISTORY: &str = "/feed/history";
/// The viewer's playlists.
pub const PLAYLISTS: &str = "/feed/playlists";
/// Where signing in starts; see [`sign_in`].
pub const SIGN_IN: &str = "/signin";
/// Where Google posts the person back, signed in: the redirect URI registered with Google.
pub const SIGNED_IN_WITH_GOOGLE: &str = "/signin/google";
/// Where a sign-in that did not work ends, with `?why=`[`SIGN_IN_UNAVAILABLE`],
/// [`SIGN_IN_REFUSED`] or [`SIGN_IN_TOO_OFTEN`].
pub const SIGN_IN_FAILED: &str = "/signin/failed";
/// Why a sign-in failed: the server is not set up to sign anyone in.
pub const SIGN_IN_UNAVAILABLE: &str = "unavailable";
/// Why a sign-in failed: Google did not vouch for the person, or they cancelled.
pub const SIGN_IN_REFUSED: &str = "refused";
/// Why a sign-in failed: this browser's address tried too often of late.
pub const SIGN_IN_TOO_OFTEN: &str = "too_often";
/// The signed-in person's settings: their channels and invitations, appearance, language,
/// personal tokens.
pub const ACCOUNT: &str = "/account";
/// Where the settings list the person's channels, and make new ones.
pub const ACCOUNT_CHANNELS: &str = "/account#channels";
/// The studio's Content tab: the channel's videos, and uploading. Each tab of the studio, and the
/// upload page, is about the channel [`in_studio`] names, or else the one the person acts as.
pub const STUDIO: &str = "/studio";
/// The studio's Customization tab: the channel's name, description and photo.
pub const STUDIO_CUSTOMIZE: &str = "/studio/customize";
/// The studio's Permissions tab: who runs the channel, as what, and whom it invited.
pub const STUDIO_PERMISSIONS: &str = "/studio/permissions";
/// The query parameter that names the channel of a page of the studio.
pub const STUDIO_CHANNEL: &str = "c";
/// The administrators' page: invitations and accounts.
pub const ADMIN: &str = "/admin";
/// The page that uploads a video.
pub const UPLOAD: &str = "/upload";

/// The page `page` of the studio (or the upload page), about `channel`.
pub fn in_studio(page: &str, channel: ChannelId) -> String {
    format!("{page}?{STUDIO_CHANNEL}={channel}")
}

/// The address that signs the person in, then brings them back to `next` (a path of this site).
pub fn sign_in(next: &str) -> String {
    format!("{SIGN_IN}?next={}", encode_component(next))
}

/// `next` if it is a path of this site (`/…`, never `//…` or `/\…`, which browsers read as
/// another host), else the home page: where to go after signing in.
pub fn local_path(next: &str) -> &str {
    let mut bytes = next.bytes();
    let local = bytes.next() == Some(b'/')
        && !matches!(bytes.next(), Some(b'/' | b'\\'))
        && !next.bytes().any(|b| b.is_ascii_control());
    if local { next } else { HOME }
}

/// The watch page of `video`.
pub fn watch(video: VideoId) -> String {
    format!("/watch?v={video}")
}

/// Where every embedded player lives: [`embed`] adds the video.
pub const EMBED: &str = "/embed";

/// The player of `video` alone, which any site may show in a frame, as YouTube's
/// `/embed/<id>`.
pub fn embed(video: VideoId) -> String {
    format!("{EMBED}/{video}")
}

/// Whether `path` is an embedded player's, which other sites may frame.
pub fn is_embed(path: &str) -> bool {
    path.strip_prefix(EMBED)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// The page of the channel whose handle is `handle`.
pub fn channel(handle: &str) -> String {
    format!("/@{}", encode_component(handle))
}

/// The tab `tab` (`videos`, `playlists`) of the page of the channel whose handle is `handle`.
pub fn channel_tab(handle: &str, tab: &str) -> String {
    format!("/@{}/{tab}", encode_component(handle))
}

/// The page of `list`.
pub fn playlist(list: List) -> String {
    format!("/playlist?list={list}")
}

/// The results page for `query`.
pub fn results(query: &str) -> String {
    format!("/results?search_query={}", encode_component(query))
}

/// The address a command `id` is posted to.
pub fn command(id: &str) -> String {
    format!("{COMMANDS}/{id}")
}

/// The directory that holds the media files of `video`; each file's name within it is in
/// [`media_files`].
pub fn media(video: VideoId) -> String {
    format!("/media/{video}")
}

/// The names of a video's media files within its [`media`] directory, as the transcoder writes
/// them and the server serves them.
pub mod media_files {
    /// The HLS playlist that lists the renditions.
    pub const RENDITIONS: &str = "master.m3u8";
    /// What the storyboard is: a [`crate::records::Storyboard`] as JSON.
    pub const STORYBOARD: &str = "storyboard.json";

    /// The thumbnail `width` pixels wide.
    pub fn thumbnail(width: u32) -> String {
        format!("thumbnail/{width}.jpg")
    }

    /// Sheet `index` of the storyboard, counted from 0.
    pub fn storyboard_sheet(index: u32) -> String {
        format!("storyboard/{index}.jpg")
    }
}

/// The thumbnail of `video`, `width` pixels wide (one of [`THUMBNAIL_WIDTHS`]).
pub fn thumbnail(video: VideoId, width: u32) -> String {
    format!("{}/{}", media(video), media_files::thumbnail(width))
}

/// The widths every thumbnail is made at, smallest first.
pub const THUMBNAIL_WIDTHS: [u32; 5] = [168, 246, 336, 480, 720];

/// The HLS playlist that lists the renditions of `video`.
pub fn playlist_of_renditions(video: VideoId) -> String {
    format!("{}/{}", media(video), media_files::RENDITIONS)
}

/// What the storyboard of `video` is, as JSON.
pub fn storyboard(video: VideoId) -> String {
    format!("{}/{}", media(video), media_files::STORYBOARD)
}

/// Sheet `index` of the storyboard of `video`.
pub fn storyboard_sheet(video: VideoId, index: u32) -> String {
    format!("{}/{}", media(video), media_files::storyboard_sheet(index))
}

/// Where the bytes of an upload in progress are sent, `PATCH` at an offset, and its offset
/// asked, `HEAD`.
pub fn upload(video: VideoId) -> String {
    format!("/upload/{video}")
}

/// Where channels' photos are served from: `<PHOTOS>/<channel>/<version>/<size>.jpg`.
pub const PHOTOS: &str = "/photos";

/// The photo of `channel` at its version `version`, `size` pixels square (one of
/// [`PHOTO_SIZES`]). A new photo is a new version, so each address keeps its bytes for ever.
pub fn photo(channel: ChannelId, version: &str, size: u32) -> String {
    format!(
        "{PHOTOS}/{channel}/{}/{}",
        encode_component(version),
        photo_file(size)
    )
}

/// The name of the file of a photo `size` pixels square, within its version's directory.
pub fn photo_file(size: u32) -> String {
    format!("{size}.jpg")
}

/// The sizes every photo is made at, smallest first: twice the avatars pages draw (24, 40, 88
/// and 160 CSS pixels), for screens of two pixels to the CSS pixel.
pub const PHOTO_SIZES: [u32; 4] = [48, 80, 176, 320];

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
        assert_eq!(embed(video), "/embed/AAAAAAAAAAA");
        assert!(is_embed(&embed(video)));
        assert!(!is_embed("/embedded") && !is_embed(&watch(video)));
        assert_eq!(channel("youcraft"), "/@youcraft");
        assert_eq!(
            results("chat noir & blanc"),
            "/results?search_query=chat%20noir%20%26%20blanc"
        );
        assert_eq!(command("feed.home"), "/api/v1/feed.home");
        assert_eq!(playlist(List::WatchLater), "/playlist?list=WL");
        assert_eq!(channel_tab("ada", "videos"), "/@ada/videos");
        assert_eq!(
            in_studio(STUDIO_PERMISSIONS, ChannelId::from_bits(0)),
            "/studio/permissions?c=AAAAAAAAAAA"
        );
        assert_eq!(sign_in("/watch?v=x"), "/signin?next=%2Fwatch%3Fv%3Dx");
        assert_eq!(
            thumbnail(video, 336),
            "/media/AAAAAAAAAAA/thumbnail/336.jpg"
        );
        assert_eq!(
            playlist_of_renditions(video),
            "/media/AAAAAAAAAAA/master.m3u8"
        );
        assert_eq!(
            storyboard_sheet(video, 2),
            "/media/AAAAAAAAAAA/storyboard/2.jpg"
        );
    }

    #[test]
    fn only_paths_of_this_site_are_local() {
        assert_eq!(local_path("/watch?v=x"), "/watch?v=x");
        assert_eq!(local_path("/"), "/");
        for elsewhere in [
            "//evil.example",
            "/\\evil.example",
            "https://evil.example",
            "",
            "/a\nb",
        ] {
            assert_eq!(local_path(elsewhere), HOME, "{elsewhere:?}");
        }
    }

    #[test]
    fn components_encode_every_reserved_byte() {
        assert_eq!(encode_component("a/b?c=d#é"), "a%2Fb%3Fc%3Dd%23%C3%A9");
        assert_eq!(encode_component("A-z_0.9~"), "A-z_0.9~");
    }
}
