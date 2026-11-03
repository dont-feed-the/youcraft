//! Who sent a request: the caller its credential proves, and the preferences to render with.
//!
//! A program proves itself with `Authorization: Bearer <token>`; a browser with its session
//! cookie. Either is looked up by its hash, so a stale one simply proves nothing. A signed-in
//! person's preferences are their account's; a visitor's are the cookies their browser wrote.
//! [`layer`] resolves the visitor once per request, before any page or command runs, and has
//! the browser forget a session cookie that proves nothing any more. It also gives the caller
//! the [`ViewerKey`] by which the view counter tells one viewer from another.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::HeaderMap;
use axum::http::header::{AUTHORIZATION, SET_COOKIE, USER_AGENT};
use axum::middleware::Next;
use axum::response::Response;
use sha2::{Digest, Sha256};
use youcraft_api::AccountId;
use youcraft_api::commands::account::{Preferences, Theme};
use youcraft_store::accounts::Standing;

use crate::cookies;
use crate::secrets::hash;
use crate::site::{Caller, Credential, Site, ViewerKey, now};

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
        Some((standing, credential)) => {
            let viewer = viewer_key(site, Some(standing.account), headers);
            signed_in(&standing, credential, viewer)
        }
        None => Visitor {
            caller: Caller {
                viewer: viewer_key(site, None, headers),
                ..Caller::ANONYMOUS
            },
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

fn signed_in(standing: &Standing, credential: Credential, viewer: ViewerKey) -> Visitor {
    Visitor {
        caller: Caller::of(standing, credential, viewer),
        preferences: standing.preferences.clone(),
        stale_session: false,
    }
}

/// Who the visitor is to the view counter: their account, or else the address the proxy saw
/// them come from (`X-Forwarded-For`) and their browser, hashed with the site's salt.
fn viewer_key(site: &Site, account: Option<AccountId>, headers: &HeaderMap) -> ViewerKey {
    let mut hasher = Sha256::new();
    hasher.update(site.salt());
    match account {
        Some(account) => {
            hasher.update(b"account\0");
            hasher.update(account.bits().to_be_bytes());
        }
        None => {
            let header = |name| headers.get(name).map_or(&[][..], |value| value.as_bytes());
            let address = header(FORWARDED_FOR)
                .split(|&b| b == b',')
                .next()
                .unwrap_or_default();
            hasher.update(b"visitor\0");
            hasher.update(address.trim_ascii());
            hasher.update(b"\0");
            hasher.update(header(USER_AGENT.as_str()));
        }
    }
    ViewerKey(hasher.finalize().into())
}

/// The header in which the proxy names the address a request came from.
const FORWARDED_FOR: &str = "x-forwarded-for";

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
