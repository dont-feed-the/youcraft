//! Who sent a request: the caller its credential proves, and the preferences to render with.
//!
//! A program proves itself with `Authorization: Bearer <token>`; a browser with its session
//! cookie. Either is looked up by its hash, so a stale one simply proves nothing. A signed-in
//! person's preferences are their account's; a visitor's are the cookies their browser wrote.
//! [`layer`] resolves the visitor once per request, before any page or command runs, and has
//! the browser forget a session cookie that proves nothing any more.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::HeaderMap;
use axum::http::header::{AUTHORIZATION, SET_COOKIE};
use axum::middleware::Next;
use axum::response::Response;
use youcraft_api::commands::account::{Preferences, Theme};
use youcraft_store::accounts::Standing;

use crate::cookies;
use crate::secrets::hash;
use crate::site::{Caller, Credential, Site, now};

/// Who sent a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Visitor {
    /// Who runs the commands of the request.
    pub caller: Caller,
    /// How to render its pages.
    pub preferences: Preferences,
    /// Whether it carried a session cookie that proves nothing.
    pub stale_session: bool,
}

/// The visitor whose request carries `headers`.
pub async fn of(site: &Site, headers: &HeaderMap) -> Visitor {
    let bearer = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim);
    let session = cookies::read(headers, cookies::SESSION);
    let found = match (bearer, session) {
        (Some(token), _) => {
            let hash = hash(token);
            let standing = site.db(move |store| store.token(&hash, now())).await;
            standing.ok().flatten().map(|s| (s, Credential::Token))
        }
        (None, Some(secret)) => {
            let hash = hash(secret);
            let standing = site.db(move |store| store.session(&hash, now())).await;
            standing
                .ok()
                .flatten()
                .map(|s| (s, Credential::Session(hash)))
        }
        (None, None) => None,
    };
    match found {
        Some((standing, credential)) => signed_in(&standing, credential),
        None => Visitor {
            caller: Caller::ANONYMOUS,
            preferences: Preferences {
                language: cookies::read(headers, youcraft_api::cookies::LANGUAGE)
                    .map(str::to_owned),
                theme: cookies::read(headers, youcraft_api::cookies::THEME)
                    .and_then(Theme::from_name)
                    .unwrap_or_default(),
            },
            stale_session: bearer.is_none() && session.is_some(),
        },
    }
}

fn signed_in(standing: &Standing, credential: Credential) -> Visitor {
    Visitor {
        caller: Caller::of(standing, credential),
        preferences: standing.preferences.clone(),
        stale_session: false,
    }
}

/// Resolve the visitor of `request` for the handlers after it, as a request extension.
pub async fn layer(State(site): State<Arc<Site>>, mut request: Request, next: Next) -> Response {
    let visitor = of(&site, request.headers()).await;
    let stale = visitor.stale_session;
    request.extensions_mut().insert(visitor);
    let mut response = next.run(request).await;
    if stale && let Some(removal) = cookies::remove(cookies::SESSION) {
        response.headers_mut().append(SET_COOKIE, removal);
    }
    response
}
