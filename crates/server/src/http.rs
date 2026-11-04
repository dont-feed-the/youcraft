//! The HTTP side: pages rendered by the server, commands at `/api/v1/<id>` and as MCP tools at
//! `/mcp`, the browser bundle, media, the bytes of uploads, and `/health`.
//!
//! Each page is rendered with its request's context: the caller, the language (theirs if they
//! chose one, else their browser's) and theme, the render time, a fresh nonce for the scripts,
//! and the site bound to that caller for the commands the page runs while rendering.

use std::sync::{Arc, LazyLock};

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Extension, FromRef, Path, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, head, post};
use leptos::prelude::*;
use leptos_axum::{AxumRouteListing, LeptosRoutes};
use tower_http::services::{ServeDir, ServeFile};
use youcraft_api::commands::account::Preferences;
use youcraft_api::commands::profile::PHOTO_BYTES;
use youcraft_api::{ApiError, Dispatch, Rule, routes};
use youcraft_ui::lang::{Language, RenderTime};

use crate::config::Config;
use crate::dispatch::{self, RequestDispatch};
use crate::listen::{Listener, listen, stopped};
use crate::signin;
use crate::site::{Caller, Credential, Site, now};
use crate::visitor::{self, Visitor};

/// The most bytes a command's input has: a photo in base64, the largest input, and room for the
/// JSON around it.
const COMMAND_BYTES: usize = PHOTO_BYTES / 3 * 4 + (64 << 10);

/// What every handler can reach.
#[derive(Clone)]
struct AppState {
    options: LeptosOptions,
    site: Arc<Site>,
}

impl FromRef<AppState> for LeptosOptions {
    fn from_ref(state: &AppState) -> LeptosOptions {
        state.options.clone()
    }
}

/// Serve the site until it is told to stop.
pub async fn serve(site: Arc<Site>, config: &Config) -> Result<(), String> {
    let app = router(site, config);
    match listen(config.address).await? {
        Listener::Unix(socket) => {
            tracing::info!("serving the socket systemd handed over");
            axum::serve(socket, app)
                .with_graceful_shutdown(stopped())
                .await
        }
        Listener::Tcp(socket) => {
            tracing::info!("serving http://{}", config.address);
            // Small responses (playlists, audio segments) go out at once rather than wait for
            // the delayed acknowledgement of the one before, which costs 40 ms each.
            let socket = axum::serve::ListenerExt::tap_io(socket, |stream| {
                if let Err(error) = stream.set_nodelay(true) {
                    tracing::warn!("TCP_NODELAY: {error}");
                }
            });
            axum::serve(socket, app)
                .with_graceful_shutdown(stopped())
                .await
        }
    }
    .map_err(|e| format!("serve: {e}"))
}

/// The pages of the app, listed once per process.
///
/// Leptos lists them by rendering the app with resource loading suspended, and that suspension is
/// one flag for the whole process (leptos-rs/leptos#4773): a resource that first polls while any
/// thread lists the pages waits forever, and its page with it. Listed here once, before any router
/// exists and so before any page renders, the window never overlaps a rendering, however many
/// sites a process opens, as the tests do in parallel.
static PAGES: LazyLock<Vec<AxumRouteListing>> =
    LazyLock::new(|| leptos_axum::generate_route_list(youcraft_ui::App));

/// The routes of the site. Pages, commands and signing in run after the visitor is resolved
/// ([`visitor::layer`]), and so do media, which only some may watch, and uploads; the bundle and
/// icon are files that need no visitor.
fn router(site: Arc<Site>, config: &Config) -> Router {
    let site_root = config.site.to_string_lossy().into_owned();
    let options = LeptosOptions::builder()
        .output_name("youcraft")
        .site_root(site_root)
        .site_pkg_dir("pkg")
        .build();
    let state = AppState {
        options: options.clone(),
        site: Arc::clone(&site),
    };
    let pages = PAGES.clone();
    let context = {
        let site = Arc::clone(&site);
        move || provide_request_context(&site)
    };
    let shell = {
        let options = options.clone();
        move || youcraft_ui::shell(options.clone())
    };
    let signing_in = Router::new()
        .route(routes::SIGN_IN, get(signin::start))
        .route(routes::SIGNED_IN_WITH_GOOGLE, post(signin::back))
        .with_state(Arc::clone(&site));
    let visited = Router::new()
        .route(
            "/api/v1/{id}",
            post(command).layer(DefaultBodyLimit::max(COMMAND_BYTES)),
        )
        .route(
            routes::MCP,
            post(mcp).layer(DefaultBodyLimit::max(COMMAND_BYTES)),
        )
        .route("/media/{video}/{*file}", get(media))
        .route("/photos/{channel}/{version}/{file}", get(photo))
        .route("/upload/{video}", head(upload_offset).patch(upload_piece))
        .leptos_routes_with_context(&state, pages, context.clone(), shell.clone())
        .fallback(leptos_axum::render_app_to_stream_with_context(
            context, shell,
        ))
        .with_state(state)
        .merge(signing_in)
        .layer(axum::middleware::from_fn_with_state(
            Arc::clone(&site),
            visitor::layer,
        ));
    Router::new()
        .route(routes::HEALTH, get(|| async { "ok" }))
        .nest_service("/pkg", ServeDir::new(config.site.join("pkg")))
        .route_service(
            "/favicon.svg",
            ServeFile::new(config.site.join("favicon.svg")),
        )
        .merge(visited)
}

/// Give the page being rendered its request's context: the visitor's language (their choice,
/// else their browser's) and theme, the render time, a nonce, and the commands as they may run
/// them.
fn provide_request_context(site: &Arc<Site>) {
    let parts = use_context::<axum::http::request::Parts>();
    let visitor = parts
        .as_ref()
        .and_then(|parts| parts.extensions.get::<Visitor>().cloned());
    let (caller, preferences) = visitor.map_or_else(
        || (Caller::ANONYMOUS, Preferences::default()),
        |visitor| (visitor.caller, visitor.preferences),
    );
    let accept = parts
        .as_ref()
        .and_then(|parts| parts.headers.get(header::ACCEPT_LANGUAGE))
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    provide_context(Language(youcraft_i18n::negotiate(
        preferences.language.as_deref(),
        accept,
    )));
    provide_context(preferences.theme);
    provide_context(RenderTime(now()));
    leptos::nonce::provide_nonce();
    let dispatch: Arc<dyn Dispatch> = Arc::new(RequestDispatch {
        site: Arc::clone(site),
        caller,
    });
    provide_context(dispatch);
}

/// `POST /api/v1/<id>`: run the command on the JSON body and answer its JSON, or the error with
/// its status.
///
/// The body must be declared JSON, which a page of another site can only send after a CORS
/// preflight the site never grants; and a browser's session counts only for a request from the
/// site's own origin. Together they keep other sites from acting with a visitor's cookie.
async fn command(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(visitor): Extension<Visitor>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let answer = match checked(&state.site, &visitor, &headers, &body) {
        Ok(input) => dispatch::run(&state.site, visitor.caller, &id, input).await,
        Err(error) => Err(error),
    };
    match answer {
        Ok(value) => axum::Json(value).into_response(),
        Err(error) => error_response(&error),
    }
}

/// `POST /mcp`: one message of the Model Context Protocol.
async fn mcp(
    State(state): State<AppState>,
    Extension(visitor): Extension<Visitor>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    crate::mcp::answer(&state.site, visitor.caller, &headers, &body).await
}

/// `error` as JSON, with its status, and when to try again if it says.
pub fn error_response(error: &ApiError) -> Response {
    let status = StatusCode::from_u16(error.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut response = (status, axum::Json(error)).into_response();
    if let ApiError::TooOften { retry_after } = error {
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, (*retry_after).into());
    }
    response
}

/// `GET /media/<video>/<file>`.
async fn media(
    State(state): State<AppState>,
    Path((video, file)): Path<(String, String)>,
    Extension(visitor): Extension<Visitor>,
    request: Request,
) -> Response {
    crate::media::serve(&state.site, visitor.caller, &video, &file, request).await
}

/// `GET /photos/<channel>/<version>/<file>`.
async fn photo(
    State(state): State<AppState>,
    Path((channel, version, file)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    crate::media::serve_photo(&state.site, (&channel, &version, &file), request).await
}

/// `HEAD /upload/<video>`.
async fn upload_offset(
    State(state): State<AppState>,
    Path(video): Path<String>,
    Extension(visitor): Extension<Visitor>,
) -> Response {
    crate::uploads::offset(&state.site, visitor.caller, &video).await
}

/// `PATCH /upload/<video>`: a piece of an upload, which a browser's session may send only from
/// the site's own pages, as it may post commands. Its body is read only once all is checked.
async fn upload_piece(
    State(state): State<AppState>,
    Path(video): Path<String>,
    Extension(visitor): Extension<Visitor>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    if let Err(error) = from_own_pages(&state.site, &visitor, &headers) {
        return error_response(&error);
    }
    crate::uploads::receive(&state.site, visitor.caller, &video, &headers, body).await
}

/// Refuse a request that a browser's session sends from another site's page.
fn from_own_pages(site: &Site, visitor: &Visitor, headers: &HeaderMap) -> Result<(), ApiError> {
    if let Credential::Session(_) = visitor.caller.credential {
        let origin = headers
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok());
        if origin != Some(site.config().origin.as_str()) {
            return Err(ApiError::Forbidden);
        }
    }
    Ok(())
}

/// The input of a command request, if its kind and origin let it run.
fn checked(
    site: &Site,
    visitor: &Visitor,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<serde_json::Value, ApiError> {
    let json = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("application/json"));
    if !json {
        return Err(ApiError::invalid("content-type", Rule::NotOffered));
    }
    from_own_pages(site, visitor, headers)?;
    if body.is_empty() {
        Ok(serde_json::json!({}))
    } else {
        serde_json::from_slice(body).map_err(|e| ApiError::schema(&e))
    }
}

#[cfg(test)]
pub mod tests {
    //! The site over HTTP, with an in-memory database and the test key standing in for Google's.

    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use youcraft_store::Store;

    use super::*;
    use crate::config::Google;
    use crate::cookies;
    use crate::google;

    /// The client ID the test site has at Google.
    pub const CLIENT: &str = "test-client";
    /// The origin of the test site.
    pub const ORIGIN: &str = "https://youcraft.test";

    /// A test site and its routes, where only those invited publish.
    pub fn site() -> (Arc<Site>, Router) {
        site_where(youcraft_api::Grant::Viewer)
    }

    /// A test site and its routes, where every account has at least `signed_in_role`.
    pub fn site_where(signed_in_role: youcraft_api::Grant) -> (Arc<Site>, Router) {
        site_of(|config| config.signed_in_role = signed_in_role)
    }

    /// A test site and its routes, configured as [`Config::testing`] says and `change` changes,
    /// with Google's test key in a file of its own.
    pub fn site_of(change: impl FnOnce(&mut Config)) -> (Arc<Site>, Router) {
        let keys = std::env::temp_dir().join(format!(
            "youcraft-test-keys-{}.json",
            crate::secrets::random_bits()
        ));
        std::fs::write(&keys, google::tests::keys()).unwrap();
        let mut config = Config::testing(std::env::temp_dir());
        config.origin = ORIGIN.to_owned();
        config.google = Some(Google {
            client_id: CLIENT.to_owned(),
            keys,
        });
        change(&mut config);
        let site = Arc::new(Site::with(Store::in_memory().unwrap(), config.clone()));
        let routes = router(Arc::clone(&site), &config);
        (site, routes)
    }

    /// The answer to `request`: status, headers and body.
    pub async fn send(routes: &Router, request: Request<Body>) -> (StatusCode, HeaderMap, String) {
        let response = routes.clone().oneshot(request).await.unwrap();
        let (parts, body) = response.into_parts();
        let body = body.collect().await.unwrap().to_bytes();
        (
            parts.status,
            parts.headers,
            String::from_utf8_lossy(&body).into_owned(),
        )
    }

    /// The value the response sets for the cookie `name`.
    pub fn set_cookie<'h>(headers: &'h HeaderMap, name: &str) -> Option<&'h str> {
        headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
            .and_then(|rest| rest.split(';').next())
    }

    /// A command posted as the site's own pages post it, with `cookie`.
    pub fn command_request(id: &str, input: &str, cookie: &str) -> Request<Body> {
        Request::post(routes::command(id))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ORIGIN, ORIGIN)
            .header(header::COOKIE, cookie)
            .body(Body::from(input.to_owned()))
            .unwrap()
    }

    /// Sign `sub` in through the whole flow; the session cookie, as `name=value`.
    pub async fn sign_in(routes: &Router, sub: &str) -> String {
        let start = Request::get("/signin?next=%2Fwatch%3Fv%3Dx")
            .body(Body::empty())
            .unwrap();
        let (status, headers, _) = send(routes, start).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let location = headers[header::LOCATION].to_str().unwrap();
        assert!(location.starts_with("https://accounts.google.com/"));
        assert!(location.contains("redirect_uri=https%3A%2F%2Fyoucraft.test%2Fsignin%2Fgoogle"));
        let secret = set_cookie(&headers, cookies::SIGN_IN).unwrap().to_owned();
        let nonce = google::nonce_of(&secret);
        assert!(location.contains(&format!("nonce={nonce}")));
        let mut claims = google::tests::claims(sub, CLIENT, &nonce);
        claims["iat"] = now().into();
        claims["exp"] = (now() + 3600).into();
        let token = google::tests::sign(&claims, "RS256");
        let back = Request::post(routes::SIGNED_IN_WITH_GOOGLE)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(header::COOKIE, format!("{}={secret}", cookies::SIGN_IN))
            .body(Body::from(format!(
                "id_token={token}&state=%2Fwatch%3Fv%3Dx"
            )))
            .unwrap();
        let (status, headers, _) = send(routes, back).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(headers[header::LOCATION], "/watch?v=x");
        let session = set_cookie(&headers, cookies::SESSION).unwrap();
        format!("{}={session}", cookies::SESSION)
    }

    #[tokio::test]
    async fn signing_in_opens_a_session_that_signing_out_closes() {
        let (site, routes) = site();
        site.store()
            .invite("ada@example.com", youcraft_api::Grant::Creator, 1)
            .unwrap();
        let cookie = sign_in(&routes, "ada").await;
        let (status, _, body) = send(&routes, command_request("account.me", "{}", &cookie)).await;
        assert_eq!(status, StatusCode::OK);
        let me: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(me["role"], "creator", "{body}");
        assert_eq!(me["channel"]["handle"], "adalovelace");
        let (status, _, _) = send(&routes, command_request("account.signout", "{}", &cookie)).await;
        assert_eq!(status, StatusCode::OK);
        let (_, headers, body) = send(&routes, command_request("account.me", "{}", &cookie)).await;
        assert_eq!(body, "null");
        assert_eq!(
            set_cookie(&headers, cookies::SESSION),
            Some(""),
            "the stale cookie goes"
        );
    }

    #[tokio::test]
    async fn a_token_for_another_attempt_signs_nobody_in() {
        let (_, routes) = site();
        let mut claims = google::tests::claims("ada", CLIENT, &google::nonce_of("theirs"));
        claims["iat"] = now().into();
        claims["exp"] = (now() + 3600).into();
        let token = google::tests::sign(&claims, "RS256");
        let back = Request::post(routes::SIGNED_IN_WITH_GOOGLE)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(header::COOKIE, format!("{}=mine", cookies::SIGN_IN))
            .body(Body::from(format!(
                "id_token={token}&state=%2F%2Fevil.example"
            )))
            .unwrap();
        let (status, headers, _) = send(&routes, back).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(
            headers[header::LOCATION],
            "/signin/failed?why=refused&next=%2F"
        );
        assert_eq!(set_cookie(&headers, cookies::SESSION), None);
    }

    #[tokio::test]
    async fn a_session_counts_only_from_the_sites_own_pages() {
        let (_, routes) = site();
        let cookie = sign_in(&routes, "ada").await;
        let elsewhere = Request::post(routes::command("token.list"))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ORIGIN, "https://evil.example")
            .header(header::COOKIE, &cookie)
            .body(Body::from("{}"))
            .unwrap();
        assert_eq!(send(&routes, elsewhere).await.0, StatusCode::FORBIDDEN);
        let form = Request::post(routes::command("token.list"))
            .header(header::CONTENT_TYPE, "text/plain")
            .header(header::ORIGIN, ORIGIN)
            .header(header::COOKIE, &cookie)
            .body(Body::from("{}"))
            .unwrap();
        assert_eq!(
            send(&routes, form).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let own = command_request("token.list", "{}", &cookie);
        assert_eq!(send(&routes, own).await.0, StatusCode::OK);
    }

    #[tokio::test]
    async fn a_personal_token_acts_as_its_owner_from_anywhere() {
        let (_, routes) = site();
        let cookie = sign_in(&routes, "ada").await;
        let made = command_request("token.create", r#"{"name": "laptop"}"#, &cookie);
        let (status, _, body) = send(&routes, made).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let created: serde_json::Value = serde_json::from_str(&body).unwrap();
        let secret = created["secret"].as_str().unwrap();
        assert!(secret.starts_with("yct_"));
        let program = |secret: &str| {
            Request::post(routes::command("account.me"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {secret}"))
                .body(Body::from("{}"))
                .unwrap()
        };
        let (_, _, body) = send(&routes, program(secret)).await;
        assert!(body.contains("adalovelace"), "{body}");
        let (_, _, body) = send(&routes, program("yct_wrong")).await;
        assert_eq!(body, "null");
        let anonymous = Request::post(routes::command("token.list"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .unwrap();
        assert_eq!(send(&routes, anonymous).await.0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn administrators_cannot_demote_or_block_themselves() {
        let (site, routes) = site();
        site.store()
            .invite("ada@example.com", youcraft_api::Grant::Admin, 1)
            .unwrap();
        let ada = sign_in(&routes, "ada").await;
        let bob = sign_in(&routes, "bob").await;
        let block = |handle: &str, cookie: &str| {
            command_request(
                "admin.block",
                &format!(r#"{{"handle": "{handle}", "blocked": true}}"#),
                cookie,
            )
        };
        assert_eq!(
            send(&routes, block("@adalovelace", &bob)).await.0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            send(&routes, block("@adalovelace", &ada)).await.0,
            StatusCode::CONFLICT
        );
        let me = send(&routes, command_request("account.me", "{}", &bob))
            .await
            .2;
        let me: serde_json::Value = serde_json::from_str(&me).unwrap();
        let bob_handle = me["channel"]["handle"].as_str().unwrap();
        assert_ne!(bob_handle, "adalovelace", "Bob is called Ada Lovelace too");
        assert_eq!(
            send(&routes, block(bob_handle, &ada)).await.0,
            StatusCode::OK
        );
        let (status, _, _) = send(
            &routes,
            command_request("playlist.create", r#"{"title": "x"}"#, &bob),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "blocked people may look, not act toward others"
        );
    }

    #[tokio::test]
    async fn a_blocked_account_takes_back_what_is_its_own() {
        use serde_json::json;

        let (site, routes) = site();
        site.store()
            .invite("ada@example.com", youcraft_api::Grant::Admin, 1)
            .unwrap();
        let ada = sign_in(&routes, "ada").await;
        let bob = sign_in(&routes, "bob").await;
        let (_, ada_me) = answer(&routes, "account.me", json!({}), &ada).await;
        let channel = ada_me["channel"]["id"].clone();
        let video = ready_video(&site, serde_json::from_value(channel.clone()).unwrap());
        let rate = |rating: &str| json!({ "video": video, "rating": rating });
        let subscribe = |subscribed: bool| json!({ "channel": channel, "subscribed": subscribed });
        for (id, input) in [
            ("video.rate", rate("like")),
            ("channel.subscribe", subscribe(true)),
            ("video.view", json!({ "video": video })),
            ("playlist.create", json!({ "title": "Mine" })),
        ] {
            assert_eq!(
                answer(&routes, id, input, &bob).await.0,
                StatusCode::OK,
                "{id}"
            );
        }
        let (_, bob_me) = answer(&routes, "account.me", json!({}), &bob).await;
        let block = json!({ "handle": bob_me["channel"]["handle"], "blocked": true });
        assert_eq!(
            answer(&routes, "admin.block", block, &ada).await.0,
            StatusCode::OK
        );

        for (id, input) in [
            ("video.rate", rate("dislike")),
            ("channel.subscribe", subscribe(true)),
            ("playlist.create", json!({ "title": "More" })),
            ("comment.create", json!({ "video": video, "body": "Hi" })),
        ] {
            let (status, _) = answer(&routes, id, input, &bob).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{id} acts toward others");
        }
        let (_, lists) = answer(&routes, "playlist.list", json!({}), &bob).await;
        let mine = lists.as_array().unwrap().last().unwrap()["list"].clone();
        let (_, made) = answer(&routes, "token.create", json!({ "name": "export" }), &bob).await;
        for (id, input) in [
            ("history.list", json!({})),
            ("history.pause", json!({ "paused": true })),
            ("history.clear", json!({})),
            ("token.list", json!({})),
            ("token.revoke", json!({ "token": made["token"]["id"] })),
            (
                "account.settings",
                json!({ "language": "fr", "theme": "dark" }),
            ),
            ("video.rate", rate("none")),
            ("channel.subscribe", subscribe(false)),
            (
                "playlist.delete",
                json!({ "playlist": mine.as_str().unwrap().trim_start_matches("PL") }),
            ),
        ] {
            let (status, answered) = answer(&routes, id, input, &bob).await;
            assert_eq!(
                status,
                StatusCode::OK,
                "{id} is Bob's to manage: {answered}"
            );
        }
        let (_, page) = answer(&routes, "video.get", json!({ "video": video }), &ada).await;
        assert_eq!(page["likes"], 0, "{page}");
    }

    /// A public video of `channel`, ready to watch.
    fn ready_video(site: &Site, channel: youcraft_api::ChannelId) -> youcraft_api::VideoId {
        use youcraft_api::records::Visibility;
        use youcraft_api::time::MediaDuration;
        use youcraft_store::videos::NewVideo;

        let video = youcraft_api::VideoId::from_bits(42);
        site.store()
            .insert_ready_video(&NewVideo {
                id: video,
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
        video
    }

    #[tokio::test]
    async fn an_open_site_lets_every_account_publish_but_the_blocked() {
        use serde_json::json;

        let (site, routes) = site_where(youcraft_api::Grant::Creator);
        site.store()
            .invite("ada@example.com", youcraft_api::Grant::Admin, 1)
            .unwrap();
        let ada = sign_in(&routes, "ada").await;
        let bob = sign_in(&routes, "bob").await;
        let (_, ada_me) = answer(&routes, "account.me", json!({}), &ada).await;
        assert_eq!(ada_me["role"], "admin", "the floor lowers nobody");
        let (_, bob_me) = answer(&routes, "account.me", json!({}), &bob).await;
        assert_eq!(bob_me["role"], "creator", "{bob_me}");
        let (_, policy) = answer(&routes, "site.policy", json!({}), "").await;
        assert_eq!(policy["signed_in_role"], "creator");
        let invite = |role: &str| json!({ "email": "eve@example.com", "role": role });
        assert_eq!(
            answer(&routes, "invite.create", invite("creator"), &ada)
                .await
                .0,
            StatusCode::UNPROCESSABLE_ENTITY,
            "everyone is a creator already"
        );
        assert_eq!(
            answer(&routes, "invite.create", invite("admin"), &ada)
                .await
                .0,
            StatusCode::OK
        );
        let upload = json!({ "title": "Mine", "size": 5 });
        let (status, _) = answer(&routes, "upload.create", upload.clone(), &bob).await;
        assert_eq!(status, StatusCode::OK);
        let handle = bob_me["channel"]["handle"].clone();
        let block = json!({ "handle": handle, "blocked": true });
        assert_eq!(
            answer(&routes, "admin.block", block, &ada).await.0,
            StatusCode::OK
        );
        let (_, bob_me) = answer(&routes, "account.me", json!({}), &bob).await;
        assert_eq!(
            bob_me["role"], "viewer",
            "a blocked account keeps its own role"
        );
        let (status, _) = answer(&routes, "upload.create", upload, &bob).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn a_piece_of_an_upload_is_not_read_before_it_is_allowed() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let (_, routes) = site();
        let read = Arc::new(AtomicBool::new(false));
        let body = {
            let read = Arc::clone(&read);
            Body::from_stream(futures_util::stream::once(async move {
                read.store(true, Ordering::SeqCst);
                Ok::<_, std::io::Error>(Bytes::from(vec![0u8; 1 << 20]))
            }))
        };
        let piece = Request::patch(routes::upload(youcraft_api::VideoId::from_bits(5)))
            .header("upload-offset", "0")
            .body(body)
            .unwrap();
        assert_eq!(send(&routes, piece).await.0, StatusCode::UNAUTHORIZED);
        assert!(
            !read.load(Ordering::SeqCst),
            "a stranger's megabyte is never read"
        );
    }

    #[tokio::test]
    async fn callers_change_the_site_only_so_often() {
        use crate::config::WriteRate;

        let (_, routes) = site_of(|config| {
            config.writes = WriteRate {
                burst: 2,
                interval: Some(std::time::Duration::from_secs(60)),
            }
        });
        let ada = sign_in(&routes, "ada").await;
        let token = || command_request("token.create", r#"{"name": "x"}"#, &ada);
        for _ in 0..2 {
            assert_eq!(send(&routes, token()).await.0, StatusCode::OK);
        }
        let (status, headers, body) = send(&routes, token()).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
        assert_eq!(headers[header::RETRY_AFTER], "60");
        assert!(body.contains("too_often"), "{body}");
        let list = command_request("token.list", "{}", &ada);
        assert_eq!(
            send(&routes, list).await.0,
            StatusCode::OK,
            "reading costs nothing"
        );
        sign_in(&routes, "bob").await;
        let again = Request::post(routes::SIGNED_IN_WITH_GOOGLE)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("id_token=x&state=%2F"))
            .unwrap();
        let (status, headers, _) = send(&routes, again).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(
            headers[header::LOCATION],
            "/signin/failed?why=too_often&next=%2F",
            "an address signs in only so often too"
        );
    }

    #[tokio::test]
    async fn every_account_customizes_its_channel_and_admins_moderate_them() {
        use serde_json::json;

        let (site, routes) = site();
        site.store()
            .invite("ada@example.com", youcraft_api::Grant::Admin, 1)
            .unwrap();
        let ada = sign_in(&routes, "ada").await;
        let bob = sign_in(&routes, "bob").await;
        let (_, bob_me) = answer(&routes, "account.me", json!({}), &bob).await;
        let handle = bob_me["channel"]["handle"].as_str().unwrap().to_owned();
        let change = json!({ "name": "  Bobby ", "description": "Waves, mostly." });
        let (status, profile) = answer(&routes, "channel.update", change, &bob).await;
        assert_eq!(status, StatusCode::OK, "a viewer edits their own channel");
        assert_eq!(profile["card"]["name"], "Bobby");
        assert_eq!(
            profile["card"]["handle"],
            handle.as_str(),
            "the handle stays"
        );
        assert_eq!(profile["description"], "Waves, mostly.");
        let too_long = json!({ "name": "x".repeat(51) });
        assert_eq!(
            answer(&routes, "channel.update", too_long, &bob).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let garbage = json!({ "image": "not base64!" });
        assert_eq!(
            answer(&routes, "channel.set_photo", garbage, &bob).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let huge = json!({ "image": "A".repeat(PHOTO_BYTES / 3 * 4 + 8) });
        assert_eq!(
            answer(&routes, "channel.set_photo", huge, &bob).await.0,
            StatusCode::PAYLOAD_TOO_LARGE
        );
        let (status, removed) = answer(&routes, "channel.remove_photo", json!({}), &bob).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(removed["card"]["photo"], serde_json::Value::Null);

        let target = json!({ "handle": handle });
        assert_eq!(
            answer(&routes, "admin.reset_name", target.clone(), &bob)
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        let (_, reset) = answer(&routes, "admin.reset_name", target.clone(), &ada).await;
        assert_eq!(
            reset["card"]["name"],
            handle.as_str(),
            "named after its handle"
        );
        let (_, found) = answer(&routes, "admin.accounts", json!({ "query": handle }), &ada).await;
        assert_eq!(found["accounts"][0]["channel"]["handle"], handle.as_str());
        assert!(
            !found.to_string().contains("@example.com"),
            "no e-mail address in the list"
        );
        let block = json!({ "handle": handle, "blocked": true });
        assert_eq!(
            answer(&routes, "admin.block", block, &ada).await.0,
            StatusCode::OK
        );
        assert_eq!(
            answer(&routes, "channel.update", json!({ "name": "Back" }), &bob)
                .await
                .0,
            StatusCode::FORBIDDEN,
            "a blocked account changes nothing"
        );
    }

    /// The JSON answer of `id` on `input`, sent with `cookie`, and its status.
    async fn answer(
        routes: &Router,
        id: &str,
        input: serde_json::Value,
        cookie: &str,
    ) -> (StatusCode, serde_json::Value) {
        let (status, _, body) = send(routes, command_request(id, &input.to_string(), cookie)).await;
        (status, serde_json::from_str(&body).unwrap_or_default())
    }

    #[tokio::test]
    async fn viewers_rate_comment_subscribe_and_keep_lists() {
        use serde_json::json;

        let (site, routes) = site();
        site.store()
            .invite("ada@example.com", youcraft_api::Grant::Creator, 1)
            .unwrap();
        let ada = sign_in(&routes, "ada").await;
        let bob = sign_in(&routes, "bob").await;
        let (_, ada_me) = answer(&routes, "account.me", json!({}), &ada).await;
        let (_, bob_me) = answer(&routes, "account.me", json!({}), &bob).await;
        let channel: youcraft_api::ChannelId =
            serde_json::from_value(ada_me["channel"]["id"].clone()).unwrap();
        let video = ready_video(&site, channel);
        let which = json!({ "video": video });

        let (_, rated) = answer(
            &routes,
            "video.rate",
            json!({ "video": video, "rating": "like" }),
            &bob,
        )
        .await;
        assert_eq!(rated["likes"], 1);
        let (status, _) = answer(
            &routes,
            "video.rate",
            json!({ "video": video, "rating": "like" }),
            "",
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "signed out, one signs in first"
        );

        let (status, comment) = answer(
            &routes,
            "comment.create",
            json!({ "video": video, "body": " Lovely. " }),
            &bob,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{comment}");
        assert_eq!(comment["body"], "Lovely.");
        let (_, comments) = answer(&routes, "comment.list", which.clone(), &ada).await;
        let first = &comments["comments"][0];
        assert!(
            first["deletable"] == true && first["editable"] == false,
            "{comments}"
        );

        let (_, subscribed) = answer(
            &routes,
            "channel.subscribe",
            json!({ "channel": channel, "subscribed": true }),
            &bob,
        )
        .await;
        assert_eq!(subscribed["subscribers"], 1);
        let (_, feed) = answer(&routes, "feed.subscriptions", json!({}), &bob).await;
        assert_eq!(feed["videos"][0]["id"], json!(video));

        for _ in 0..2 {
            let (status, _) = answer(&routes, "video.view", which.clone(), &bob).await;
            assert_eq!(status, StatusCode::OK);
        }
        let (_, page) = answer(&routes, "video.get", which.clone(), &bob).await;
        assert_eq!(page["card"]["views"], 1, "once a day per viewer");
        assert!(
            page["subscribed"] == true && page["rating"] == "like",
            "{page}"
        );
        let (_, history) = answer(&routes, "history.list", json!({}), &bob).await;
        assert_eq!(history["videos"][0]["card"]["id"], json!(video));

        let (status, _) = answer(
            &routes,
            "playlist.add",
            json!({ "list": "WL", "video": video }),
            &bob,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (_, later) = answer(&routes, "playlist.get", json!({ "list": "WL" }), &bob).await;
        assert_eq!(later["videos"][0]["id"], json!(video));
        let (status, _) = answer(&routes, "playlist.get", json!({ "list": "WL" }), "").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let (_, found) = answer(&routes, "search.videos", json!({ "query": "WAVES" }), "").await;
        assert_eq!(found["videos"][0]["id"], json!(video));

        let handle = bob_me["channel"]["handle"].clone();
        let (status, _) = answer(
            &routes,
            "channel.block",
            json!({ "handle": handle, "blocked": true }),
            &ada,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = answer(
            &routes,
            "comment.create",
            json!({ "video": video, "body": "Again" }),
            &bob,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "the creator keeps Bob from commenting"
        );
        let (_, comments) = answer(&routes, "comment.list", which, "").await;
        assert_eq!(
            comments["comments"],
            json!([]),
            "and his comments from showing"
        );
    }
}
