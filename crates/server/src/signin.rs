//! Signing in with Google: the redirect there, and the post back with an ID token.
//!
//! `GET /signin?next=…` gives the browser a fresh secret in a short-lived cookie and sends it to
//! Google with the secret's hash as the nonce (OpenID Connect's implicit flow, `id_token` posted
//! back as a form). `POST /signin/google` verifies the token against Google's keys, the client
//! ID and that nonce, so only the browser that set out can finish; then it signs the person in
//! (their account is made on their first sign-in, and an invitation of their address raises
//! its role), opens a session in a cookie, and goes back to `next`. Anything wrong ends at
//! `/signin/failed`. The address and the token never reach a log.

use std::sync::Arc;

use axum::Form;
use axum::extract::{Extension, Query, State};
use axum::http::header::{LOCATION, SET_COOKIE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use tokio::io::AsyncReadExt;
use youcraft_api::{AccountId, ChannelId, routes};
use youcraft_store::accounts::GooglePerson;

use crate::config::Google;
use crate::cookies;
use crate::google::{self, KEYS_LIMIT};
use crate::handlers::channel::{display_name, handles};
use crate::rate::Spender;
use crate::secrets::{new_secret, random_bits};
use crate::site::{Site, now};
use crate::visitor::Visitor;

/// Where Google's people sign in.
const GOOGLE_SIGN_IN: &str = "https://accounts.google.com/o/oauth2/v2/auth";

/// The query of `GET /signin`.
#[derive(Deserialize)]
pub struct Start {
    next: Option<String>,
}

/// The form Google posts back.
#[derive(Deserialize)]
pub struct Back {
    id_token: Option<String>,
    state: Option<String>,
}

/// `GET /signin`: off to Google.
pub async fn start(State(site): State<Arc<Site>>, Query(start): Query<Start>) -> Response {
    let Some(google) = &site.config().google else {
        return failed(routes::SIGN_IN_UNAVAILABLE, None);
    };
    let next = routes::local_path(start.next.as_deref().unwrap_or(routes::HOME));
    let (secret, _) = new_secret();
    let address = format!(
        "{GOOGLE_SIGN_IN}?client_id={}&redirect_uri={}&response_type=id_token&scope=openid%20email%20profile\
         &response_mode=form_post&prompt=select_account&nonce={}&state={}",
        routes::encode_component(&google.client_id),
        routes::encode_component(&redirect_uri(&site.config().origin)),
        google::nonce_of(&secret),
        routes::encode_component(next),
    );
    let cookie = cookies::set(cookies::SIGN_IN, &secret, cookies::SIGN_IN_AGE, "None");
    redirect(&address, [cookie].into_iter().flatten())
}

/// `POST /signin/google`: back from Google. Signing in spends from the rate of the browser's
/// address, as a change of the site does, before the token is verified.
pub async fn back(
    State(site): State<Arc<Site>>,
    Extension(visitor): Extension<Visitor>,
    headers: HeaderMap,
    Form(back): Form<Back>,
) -> Response {
    let Some(google) = site.config().google.clone() else {
        return failed(routes::SIGN_IN_UNAVAILABLE, None);
    };
    let next = routes::local_path(back.state.as_deref().unwrap_or(routes::HOME)).to_owned();
    if site
        .spend_from(Spender::Address(visitor.caller.address))
        .is_err()
    {
        return failed(routes::SIGN_IN_TOO_OFTEN, Some(&next));
    }
    let secret = cookies::read(&headers, cookies::SIGN_IN);
    let (Some(token), Some(secret)) = (back.id_token, secret) else {
        return failed(routes::SIGN_IN_REFUSED, Some(&next));
    };
    let person = match person(&google, &token, secret).await {
        Ok(person) => person,
        Err(why) => {
            tracing::info!("sign-in refused: {why}");
            return failed(routes::SIGN_IN_REFUSED, Some(&next));
        }
    };
    let (session, hash) = new_secret();
    let now = now();
    let signed_in = site
        .db(move |store| {
            let name = person.name.as_deref().map(display_name).unwrap_or_default();
            let ids = (
                AccountId::from_bits(random_bits()),
                ChannelId::from_bits(random_bits()),
            );
            let standing = store.sign_in_with_google(
                &GooglePerson {
                    subject: &person.subject,
                    verified_email: person.verified_email.as_deref(),
                    name: if name.is_empty() { "YouCraft" } else { &name },
                },
                ids,
                &mut handles(&name),
                now,
            )?;
            store.open_session(&hash, standing.account, now, now + cookies::SESSION_AGE)
        })
        .await;
    if signed_in.is_err() {
        return failed(routes::SIGN_IN_REFUSED, Some(&next));
    }
    let cookies = [
        cookies::set(cookies::SESSION, &session, cookies::SESSION_AGE, "Lax"),
        cookies::remove(cookies::SIGN_IN),
    ];
    redirect(&next, cookies.into_iter().flatten())
}

/// The person `token` vouches for, read with the key file of `google`.
async fn person(google: &Google, token: &str, secret: &str) -> Result<google::Person, String> {
    let file = tokio::fs::File::open(&google.keys)
        .await
        .map_err(|e| format!("{}: {e}", google.keys.display()))?;
    let mut keys = String::new();
    file.take(KEYS_LIMIT)
        .read_to_string(&mut keys)
        .await
        .map_err(|e| format!("{}: {e}", google.keys.display()))?;
    google::verify(
        token,
        &keys,
        &google.client_id,
        &google::nonce_of(secret),
        now(),
    )
    .map_err(|refusal| refusal.to_string())
}

/// Where Google sends people back to the site at `origin`.
pub fn redirect_uri(origin: &str) -> String {
    format!("{origin}{}", routes::SIGNED_IN_WITH_GOOGLE)
}

/// A `303 See Other` to `address`, setting `cookies`.
fn redirect(address: &str, cookies: impl Iterator<Item = HeaderValue>) -> Response {
    let mut response = StatusCode::SEE_OTHER.into_response();
    if let Ok(location) = HeaderValue::from_str(address) {
        response.headers_mut().insert(LOCATION, location);
    }
    for cookie in cookies {
        response.headers_mut().append(SET_COOKIE, cookie);
    }
    response
}

/// Off to the page that says why signing in failed, and offers to try again for `next`.
fn failed(why: &str, next: Option<&str>) -> Response {
    let mut address = format!("{}?why={why}", routes::SIGN_IN_FAILED);
    if let Some(next) = next {
        address.push_str(&format!("&next={}", routes::encode_component(next)));
    }
    redirect(&address, cookies::remove(cookies::SIGN_IN).into_iter())
}
