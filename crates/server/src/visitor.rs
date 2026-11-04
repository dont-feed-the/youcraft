//! Who sent a request: the caller its credential proves, and the preferences to render with.
//!
//! A program proves itself with `Authorization: Bearer <token>`; a browser with its session
//! cookie. Either is looked up by its hash, so a stale one simply proves nothing. A signed-in
//! person's preferences are their account's; a visitor's are the cookies their browser wrote.
//! [`layer`] resolves the visitor once per request, before any page or command runs, and has
//! the browser forget a session cookie that the database found to prove nothing any more; one it
//! could not look up (the database failed) proves nothing for this request only, and the
//! browser keeps it. It also gives the caller the [`ViewerKey`] by which the view counter tells
//! one viewer from another, and the [`AddressKey`] by which the rate limit tells apart those
//! signed out.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::HeaderMap;
use axum::http::header::{AUTHORIZATION, SET_COOKIE, USER_AGENT};
use axum::middleware::Next;
use axum::response::Response;
use sha2::{Digest, Sha256};
use youcraft_api::commands::account::{Preferences, Theme};
use youcraft_api::{AccountId, ApiError, Grant};
use youcraft_store::accounts::Standing;

use crate::cookies;
use crate::secrets::hash;
use crate::site::{AddressKey, Caller, Credential, Site, ViewerKey, now};

/// Who sent a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Visitor {
    /// Who runs the commands of the request.
    pub caller: Caller,
    /// How to render its pages.
    pub preferences: Preferences,
    /// Whether it carried a session cookie that the database found to prove nothing.
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
    let found: Result<Option<(Standing, Credential)>, ApiError> = match (bearer, session) {
        (Some(token), _) => {
            let hash = hash(token);
            let standing = site.db(move |store| store.token(&hash, now())).await;
            standing.map(|found| found.map(|s| (s, Credential::Token(hash))))
        }
        (None, Some(secret)) => {
            let hash = hash(secret);
            let standing = site.db(move |store| store.session(&hash, now())).await;
            standing.map(|found| found.map(|s| (s, Credential::Session(hash))))
        }
        (None, None) => Ok(None),
    };
    // A credential the database could not look up proves nothing now, and nothing against
    // itself: the next request may find it.
    let stale_session = bearer.is_none() && session.is_some() && matches!(found, Ok(None));

    let address = address_key(site, headers);
    match found {
        Ok(Some((standing, credential))) => {
            let viewer = viewer_key(site, Some(standing.account), headers);
            let mut visitor =
                signed_in(&standing, site.config().signed_in_role, credential, viewer);
            visitor.caller.address = address;
            visitor
        }
        Ok(None) | Err(_) => Visitor {
            caller: Caller {
                viewer: viewer_key(site, None, headers),
                address,
                ..Caller::ANONYMOUS
            },
            preferences: Preferences {
                language: cookies::read(headers, youcraft_api::cookies::LANGUAGE)
                    .map(str::to_owned),
                theme: cookies::read(headers, youcraft_api::cookies::THEME)
                    .and_then(Theme::from_name)
                    .unwrap_or_default(),
            },
            stale_session,
        },
    }
}

fn signed_in(
    standing: &Standing,
    floor: Grant,
    credential: Credential,
    viewer: ViewerKey,
) -> Visitor {
    Visitor {
        caller: Caller::of(standing, floor, credential, viewer),
        preferences: standing.preferences.clone(),
        stale_session: false,
    }
}

/// Who the visitor is to the view counter: their account, or else the address they came from
/// and their browser, hashed with the site's salt.
fn viewer_key(site: &Site, account: Option<AccountId>, headers: &HeaderMap) -> ViewerKey {
    let mut hasher = Sha256::new();
    hasher.update(site.salt());
    match account {
        Some(account) => {
            hasher.update(b"account\0");
            hasher.update(account.bits().to_be_bytes());
        }
        None => {
            hasher.update(b"visitor\0");
            hasher.update(address(headers));
            hasher.update(b"\0");
            hasher.update(headers.get(USER_AGENT).map_or(&[][..], |v| v.as_bytes()));
        }
    }
    ViewerKey(hasher.finalize().into())
}

/// The address the request came from, hashed with the site's salt.
fn address_key(site: &Site, headers: &HeaderMap) -> AddressKey {
    let mut hasher = Sha256::new();
    hasher.update(site.salt());
    hasher.update(b"address\0");
    hasher.update(address(headers));
    AddressKey(hasher.finalize().into())
}

/// The address the request came from, as the proxy in front of the site saw it: the last entry
/// of `X-Forwarded-For`, which the proxy writes, never an earlier one, which the client may.
/// Empty when no proxy says.
fn address(headers: &HeaderMap) -> &[u8] {
    headers
        .get(FORWARDED_FOR)
        .map_or(&[][..], |value| value.as_bytes())
        .rsplit(|&b| b == b',')
        .next()
        .unwrap_or_default()
        .trim_ascii()
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

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;
    use axum::http::header::COOKIE;
    use youcraft_api::ChannelId;
    use youcraft_store::accounts::NewAccount;

    use super::*;
    use crate::secrets::new_secret;

    /// The headers of a request that carries the session cookie `secret`.
    fn carrying(secret: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let cookie = format!("{}={secret}", cookies::SESSION);
        headers.insert(COOKIE, HeaderValue::from_str(&cookie).unwrap());
        headers
    }

    #[tokio::test]
    async fn only_a_session_the_database_lacks_is_forgotten() {
        let (site, dir) = crate::uploads::tests::site("visitor-sessions");
        let account = AccountId::from_bits(7);
        site.store()
            .create_account(&NewAccount {
                id: account,
                channel: ChannelId::from_bits(8),
                google_subject: "g7",
                email: "ada@example.com",
                name: "Ada",
                handle: "ada",
                role: Grant::Viewer,
                now: now(),
            })
            .unwrap();
        let (secret, hash) = new_secret();
        site.store()
            .open_session(&hash, account, now(), now() + 3600)
            .unwrap();
        let signed_in = of(&site, &carrying(&secret)).await;
        assert_eq!(signed_in.caller.account, Some(account));
        let gone = of(&site, &carrying("not-a-session")).await;
        assert!(gone.caller.account.is_none() && gone.stale_session);

        // The database fails to look the session up: it proves nothing now, and stays.
        let other = rusqlite::Connection::open(dir.join("youcraft.sqlite")).unwrap();
        other
            .execute_batch("ALTER TABLE sessions RENAME TO sessions_away")
            .unwrap();
        let failed = of(&site, &carrying(&secret)).await;
        assert!(failed.caller.account.is_none());
        assert!(!failed.stale_session, "a failed lookup signs nobody out");
        other
            .execute_batch("ALTER TABLE sessions_away RENAME TO sessions")
            .unwrap();
        let back = of(&site, &carrying(&secret)).await;
        assert_eq!(back.caller.account, Some(account));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_address_is_the_one_the_proxy_wrote_last() {
        let mut headers = HeaderMap::new();
        assert_eq!(address(&headers), b"");
        let forwarded = "203.0.113.9, 198.51.100.7 ".parse().unwrap();
        headers.insert(FORWARDED_FOR, forwarded);
        assert_eq!(
            address(&headers),
            b"198.51.100.7",
            "a client may write the first"
        );
    }
}
