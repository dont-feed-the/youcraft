//! The HTTP side: pages rendered by the server, commands at `/api/v1/<id>`, the browser bundle,
//! media, and `/health`.
//!
//! Each page is rendered with its request's context: the caller, the language (theirs if they
//! chose one, else their browser's) and theme, the render time, a fresh nonce for the scripts,
//! and the site bound to that caller for the commands the page runs while rendering.

use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Extension, FromRef, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use leptos::prelude::*;
use leptos_axum::LeptosRoutes;
use tower_http::services::{ServeDir, ServeFile};
use youcraft_api::commands::account::Preferences;
use youcraft_api::{ApiError, Dispatch, routes};
use youcraft_ui::lang::{Language, RenderTime};

use crate::config::Config;
use crate::dispatch::{self, RequestDispatch};
use crate::listen::{Listener, listen, stopped};
use crate::signin;
use crate::site::{Caller, Credential, Site, now};
use crate::visitor::{self, Visitor};

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
            axum::serve(socket, app)
                .with_graceful_shutdown(stopped())
                .await
        }
    }
    .map_err(|e| format!("serve: {e}"))
}

/// The routes of the site. Pages, commands and signing in run after the visitor is resolved
/// ([`visitor::layer`]); the bundle, media and icon are files that need no visitor.
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
    let pages = leptos_axum::generate_route_list(youcraft_ui::App);
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
        .route("/api/v1/{id}", post(command))
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
        .nest_service("/media", ServeDir::new(config.data.join("media")))
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
        Err(error) => {
            let status =
                StatusCode::from_u16(error.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            (status, axum::Json(error)).into_response()
        }
    }
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
        return Err(ApiError::invalid("content-type", "application/json"));
    }
    if let Credential::Session(_) = visitor.caller.credential {
        let origin = headers
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok());
        if origin != Some(site.config().origin.as_str()) {
            return Err(ApiError::Forbidden);
        }
    }
    if body.is_empty() {
        Ok(serde_json::json!({}))
    } else {
        serde_json::from_slice(body).map_err(|e| ApiError::invalid("input", &e.to_string()))
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

    /// A test site and its routes.
    pub fn site() -> (Arc<Site>, Router) {
        let keys = std::env::temp_dir().join(format!(
            "youcraft-test-keys-{}.json",
            crate::secrets::random_bits()
        ));
        std::fs::write(&keys, google::tests::keys()).unwrap();
        let config = Config {
            data: std::env::temp_dir(),
            site: std::env::temp_dir(),
            address: "127.0.0.1:0".parse().unwrap(),
            origin: ORIGIN.to_owned(),
            google: Some(Google {
                client_id: CLIENT.to_owned(),
                keys,
            }),
        };
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
            .invite("ada@example.com", youcraft_api::Role::Creator, 1)
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
            .invite("ada@example.com", youcraft_api::Role::Admin, 1)
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
            command_request("token.create", r#"{"name": "x"}"#, &bob),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "blocked people may look, not act"
        );
    }
}
