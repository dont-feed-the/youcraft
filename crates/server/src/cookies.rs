//! Cookies: reading them from a request, and the ones the server sets.
//!
//! The server's cookies are `__Host-` cookies, `Secure`, `HttpOnly`, for the whole site and no
//! other host. The session cookie is `SameSite=Lax`, so other sites' pages never post with it;
//! the sign-in cookie is `SameSite=None`, because Google posts the person back from its own
//! site, and lives ten minutes.

use axum::http::HeaderMap;
use axum::http::header::{COOKIE, HeaderValue};

/// The session of a signed-in browser.
pub const SESSION: &str = "__Host-session";
/// The secret of a sign-in under way, whose hash is the nonce Google must echo.
pub const SIGN_IN: &str = "__Host-signin";
/// How long a session lasts, in seconds: 180 days.
pub const SESSION_AGE: i64 = 180 * 86_400;
/// How long a sign-in may take, in seconds.
pub const SIGN_IN_AGE: i64 = 600;

/// The value of the cookie `name` the request carries, if it carries it once or more.
pub fn read<'h>(headers: &'h HeaderMap, name: &str) -> Option<&'h str> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|line| line.split(';'))
        .find_map(|pair| {
            let (key, value) = pair.trim().split_once('=')?;
            (key == name).then_some(value.trim_matches('"'))
        })
}

/// The `Set-Cookie` value that gives the browser `name=value` for `age` seconds, read by the
/// server alone; a negative or zero age removes it.
pub fn set(name: &str, value: &str, age: i64, same_site: &str) -> Option<HeaderValue> {
    HeaderValue::from_str(&format!(
        "{name}={value}; Path=/; Max-Age={}; HttpOnly; Secure; SameSite={same_site}",
        age.max(0)
    ))
    .ok()
}

/// The `Set-Cookie` value that removes the cookie `name`.
pub fn remove(name: &str) -> Option<HeaderValue> {
    set(name, "", 0, "Lax")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cookie_is_found_among_others() {
        let mut headers = HeaderMap::new();
        headers.append(COOKIE, HeaderValue::from_static("a=1; __Host-session=abc"));
        headers.append(COOKIE, HeaderValue::from_static("theme=dark"));
        assert_eq!(read(&headers, SESSION), Some("abc"));
        assert_eq!(read(&headers, "theme"), Some("dark"));
        assert_eq!(read(&headers, "b"), None);
    }

    #[test]
    fn server_cookies_are_host_only_secure_and_unreadable_by_scripts() {
        let value = set(SESSION, "abc", 60, "Lax").unwrap();
        assert_eq!(
            value.to_str().unwrap(),
            "__Host-session=abc; Path=/; Max-Age=60; HttpOnly; Secure; SameSite=Lax"
        );
        assert!(
            remove(SESSION)
                .unwrap()
                .to_str()
                .unwrap()
                .contains("Max-Age=0")
        );
    }
}
