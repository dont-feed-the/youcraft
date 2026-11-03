//! The HTTP side: pages rendered by the server, commands at `/api/v1/<id>`, the browser bundle,
//! media, and `/health`.
//!
//! Each page is rendered with its request's context: the caller, the language (theirs if they
//! chose one, else their browser's), the render time, a fresh nonce for the scripts, and the
//! site bound to that caller for the commands the page runs while rendering.

use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{FromRef, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use leptos::prelude::*;
use leptos_axum::LeptosRoutes;
use tower_http::services::{ServeDir, ServeFile};
use youcraft_api::{ApiError, Dispatch, routes};
use youcraft_ui::lang::{Language, RenderTime};

use crate::config::Config;
use crate::dispatch::{self, RequestDispatch};
use crate::listen::{Listener, listen, stopped};
use crate::site::{Caller, Site, now};

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

/// The routes of the site.
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
    Router::new()
        .route(routes::HEALTH, get(|| async { "ok" }))
        .route("/api/v1/{id}", post(command))
        .nest_service("/pkg", ServeDir::new(config.site.join("pkg")))
        .nest_service("/media", ServeDir::new(config.data.join("media")))
        .route_service(
            "/favicon.svg",
            ServeFile::new(config.site.join("favicon.svg")),
        )
        .leptos_routes_with_context(&state, pages, context.clone(), shell.clone())
        .fallback(leptos_axum::render_app_to_stream_with_context(
            context, shell,
        ))
        .with_state(state)
}

/// Give the page being rendered its request's context.
fn provide_request_context(site: &Arc<Site>) {
    let headers = use_context::<axum::http::request::Parts>()
        .map(|parts| parts.headers)
        .unwrap_or_default();
    let caller = caller(&headers);
    let accept = headers
        .get(header::ACCEPT_LANGUAGE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    provide_context(Language(youcraft_i18n::negotiate(None, accept)));
    provide_context(RenderTime(now()));
    leptos::nonce::provide_nonce();
    let dispatch: Arc<dyn Dispatch> = Arc::new(RequestDispatch {
        site: Arc::clone(site),
        caller,
    });
    provide_context(dispatch);
}

/// Who sent a request.
fn caller(_headers: &HeaderMap) -> Caller {
    Caller::ANONYMOUS
}

/// `POST /api/v1/<id>`: run the command on the JSON body and answer its JSON, or the error with
/// its status.
async fn command(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let input = if body.is_empty() {
        Ok(serde_json::json!({}))
    } else {
        serde_json::from_slice(&body).map_err(|e| ApiError::invalid("input", &e.to_string()))
    };
    let answer = match input {
        Ok(input) => dispatch::run(&state.site, caller(&headers), &id, input).await,
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
