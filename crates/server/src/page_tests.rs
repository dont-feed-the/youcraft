//! The pages as the server answers them: every route, signed in and signed out, left to right
//! and right to left, renders without a panic, with the status its content says (404 for what is
//! not there, 401 for what needs a sign-in, 403 for what the caller may not see) and its own
//! `<title>` in the HTML, before any script runs.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use youcraft_api::records::Visibility;
use youcraft_api::time::MediaDuration;
use youcraft_api::{ChannelId, Grant, VideoId};
use youcraft_i18n::Lang;
use youcraft_store::videos::NewVideo;

use crate::http::tests::{command_request, send, sign_in, site};
use crate::site::now;

/// A video that exists, and one that does not.
const WAVES: VideoId = VideoId::from_bits(42);
const MISSING: VideoId = VideoId::from_bits(43);

/// The page at `path` in the language `accept`, with `cookie`: its status, title and document.
async fn page(
    routes: &Router,
    path: &str,
    cookie: &str,
    accept: &str,
) -> (StatusCode, String, String) {
    let request = Request::get(path)
        .header(header::COOKIE, cookie)
        .header(header::ACCEPT_LANGUAGE, accept)
        .body(Body::empty())
        .unwrap();
    let (status, _, html) = send(routes, request).await;
    let title = html
        .split_once("<title>")
        .and_then(|(_, rest)| rest.split_once("</title>"))
        .map(|(title, _)| title.replace("&amp;", "&"))
        .unwrap_or_default();
    (status, title, html)
}

/// A site where Ada, a creator, published "Standing waves", Bob only watches and Grace runs the
/// site.
struct Published {
    routes: Router,
    /// The session cookies of Ada, Bob and Grace.
    ada: String,
    bob: String,
    grace: String,
    /// Ada's handle.
    handle: String,
}

async fn published() -> Published {
    let (site, routes) = site();
    site.store()
        .invite("ada@example.com", Grant::Creator, 1)
        .unwrap();
    site.store()
        .invite("grace@example.com", Grant::Admin, 1)
        .unwrap();
    let ada = sign_in(&routes, "ada").await;
    let bob = sign_in(&routes, "bob").await;
    let grace = sign_in(&routes, "grace").await;
    let me = send(&routes, command_request("account.me", "{}", &ada))
        .await
        .2;
    let me: serde_json::Value = serde_json::from_str(&me).unwrap();
    let channel: ChannelId = serde_json::from_value(me["channel"]["id"].clone()).unwrap();
    let handle = me["channel"]["handle"].as_str().unwrap().to_owned();
    site.store()
        .insert_ready_video(&NewVideo {
            id: WAVES,
            channel,
            title: "Standing waves",
            description: "On a string.",
            visibility: Visibility::Public,
            duration: MediaDuration::from_millis(1000),
            width: 640,
            height: 360,
            published_at: now(),
        })
        .unwrap();
    Published {
        routes,
        ada,
        bob,
        grace,
        handle,
    }
}

/// `name` as a page's title.
fn titled(name: &str) -> String {
    youcraft_ui::app::titled(name)
}

#[tokio::test]
async fn every_page_says_its_status_and_title_in_the_html() {
    let Published {
        routes,
        ada,
        bob,
        grace,
        handle,
    } = published().await;
    let channel = format!("/@{handle}");
    let channel_videos = format!("/@{handle}/videos");
    let channel_lists = format!("/@{handle}/playlists");
    let watch = format!("/watch?v={WAVES}");
    let missing = format!("/watch?v={MISSING}");
    let embed = format!("/embed/{WAVES}");
    let embed_missing = format!("/embed/{MISSING}");
    let en = |source: &'static str| titled(youcraft_i18n::t(Lang::EN, source));
    let cases: Vec<(&str, &str, StatusCode, String)> = vec![
        ("/", "", StatusCode::OK, "YouCraft".to_owned()),
        (&watch, "", StatusCode::OK, titled("Standing waves")),
        (&missing, "", StatusCode::NOT_FOUND, "YouCraft".to_owned()),
        (
            "/watch?v=bad",
            "",
            StatusCode::NOT_FOUND,
            "YouCraft".to_owned(),
        ),
        (&embed, "", StatusCode::OK, titled("Standing waves")),
        (
            &embed_missing,
            "",
            StatusCode::NOT_FOUND,
            "YouCraft".to_owned(),
        ),
        (
            "/embed/bad",
            "",
            StatusCode::NOT_FOUND,
            "YouCraft".to_owned(),
        ),
        (&channel, "", StatusCode::OK, titled("Ada Lovelace")),
        (
            &channel_videos,
            &bob,
            StatusCode::OK,
            titled("Ada Lovelace"),
        ),
        (&channel_lists, "", StatusCode::OK, titled("Ada Lovelace")),
        ("/@nobody", "", StatusCode::NOT_FOUND, "YouCraft".to_owned()),
        (
            "/results?search_query=waves",
            "",
            StatusCode::OK,
            titled("waves"),
        ),
        (
            "/feed/subscriptions",
            "",
            StatusCode::UNAUTHORIZED,
            en("Subscriptions"),
        ),
        (
            "/feed/subscriptions",
            &bob,
            StatusCode::OK,
            en("Subscriptions"),
        ),
        ("/feed/history", "", StatusCode::UNAUTHORIZED, en("History")),
        ("/feed/history", &bob, StatusCode::OK, en("History")),
        (
            "/feed/playlists",
            "",
            StatusCode::UNAUTHORIZED,
            en("Playlists"),
        ),
        ("/feed/playlists", &bob, StatusCode::OK, en("Playlists")),
        (
            "/playlist?list=WL",
            "",
            StatusCode::UNAUTHORIZED,
            "YouCraft".to_owned(),
        ),
        (
            "/playlist?list=WL",
            &bob,
            StatusCode::OK,
            titled(youcraft_i18n::t(Lang::EN, "Watch later")),
        ),
        (
            "/playlist?list=nothing",
            &bob,
            StatusCode::NOT_FOUND,
            "YouCraft".to_owned(),
        ),
        (
            "/studio",
            "",
            StatusCode::UNAUTHORIZED,
            en("Channel content"),
        ),
        ("/studio", &bob, StatusCode::OK, en("Channel content")),
        ("/studio", &ada, StatusCode::OK, en("Channel content")),
        ("/upload", &ada, StatusCode::OK, en("Upload video")),
        (
            "/studio/customize",
            "",
            StatusCode::UNAUTHORIZED,
            en("Customization"),
        ),
        (
            "/studio/customize",
            &bob,
            StatusCode::OK,
            en("Customization"),
        ),
        (
            "/studio/permissions",
            &bob,
            StatusCode::OK,
            en("Permissions"),
        ),
        (
            "/studio?c=AAAAAAAAAAA",
            &bob,
            StatusCode::NOT_FOUND,
            en("Channel content"),
        ),
        ("/admin", "", StatusCode::UNAUTHORIZED, en("Administration")),
        ("/admin", &bob, StatusCode::FORBIDDEN, en("Administration")),
        ("/admin", &grace, StatusCode::OK, en("Administration")),
        ("/account", "", StatusCode::UNAUTHORIZED, en("Settings")),
        ("/account", &bob, StatusCode::OK, en("Settings")),
        (
            "/signin/failed?why=refused",
            "",
            StatusCode::OK,
            en("Sign-in did not work"),
        ),
        (
            "/nowhere/at/all",
            "",
            StatusCode::NOT_FOUND,
            "YouCraft".to_owned(),
        ),
    ];
    for (path, cookie, status, title) in cases {
        let signed_in = if cookie.is_empty() {
            "signed out"
        } else {
            "signed in"
        };
        let (got, got_title, html) = page(&routes, path, cookie, "en").await;
        assert_eq!(got, status, "{path}, {signed_in}");
        assert_eq!(got_title, title, "{path}, {signed_in}");
        assert!(html.contains(r#"<html lang="en" dir="ltr""#), "{path}");
    }
}

#[tokio::test]
async fn pages_in_a_right_to_left_language_say_so_and_are_translated() {
    let Published {
        routes,
        bob,
        handle,
        ..
    } = published().await;
    let arabic = youcraft_i18n::i18n().from_code("ar").unwrap();
    let ar = |source: &'static str| titled(youcraft_i18n::t(arabic, source));
    let channel = format!("/@{handle}");
    let watch = format!("/watch?v={WAVES}");
    for (path, cookie, status, title) in [
        ("/", "", StatusCode::OK, "YouCraft".to_owned()),
        (watch.as_str(), "", StatusCode::OK, titled("Standing waves")),
        (channel.as_str(), "", StatusCode::OK, titled("Ada Lovelace")),
        ("/feed/history", "", StatusCode::UNAUTHORIZED, ar("History")),
        ("/feed/history", bob.as_str(), StatusCode::OK, ar("History")),
        ("/account", bob.as_str(), StatusCode::OK, ar("Settings")),
        ("/nowhere", "", StatusCode::NOT_FOUND, "YouCraft".to_owned()),
    ] {
        let (got, got_title, html) = page(&routes, path, cookie, "ar,en;q=0.5").await;
        assert_eq!(got, status, "{path}");
        assert_eq!(got_title, title, "{path}");
        assert!(html.contains(r#"<html lang="ar" dir="rtl""#), "{path}");
    }
}

#[tokio::test]
async fn a_menu_shows_its_choice_before_any_script_runs() {
    let Published { routes, grace, .. } = published().await;
    let (status, _, html) = page(&routes, "/admin", &grace, "en").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        html.contains(r#"<option value="creator" selected"#),
        "Ada's role, as the server rendered the administration"
    );
}
