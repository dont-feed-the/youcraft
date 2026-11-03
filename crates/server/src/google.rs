//! Google's ID tokens: checking one that came back from signing in.
//!
//! A token is a JWT (RFC 7519) that Google signed with RS256 under one of the keys of its JWKS
//! (RFC 7517). It vouches for a person only if its signature verifies under the key its header
//! names, Google issued it, it names this site's client ID as its audience, it has not expired,
//! and it carries the nonce this browser was given when it set out to sign in. Every part is
//! bounded before it is decoded.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rsa::traits::PublicKeyParts;
use rsa::{BigUint, Pkcs1v15Sign, RsaPublicKey};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// The most bytes of an ID token accepted.
pub const TOKEN_LIMIT: usize = 16 * 1024;
/// The most bytes of a key file read.
pub const KEYS_LIMIT: u64 = 256 * 1024;
/// The issuers Google's tokens name.
const ISSUERS: [&str; 2] = ["https://accounts.google.com", "accounts.google.com"];
/// How far the clocks of Google and the site may disagree, in seconds.
const SKEW: i64 = 300;

/// Who Google vouches for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Person {
    /// Their `sub`: who they are to Google, for good.
    pub subject: String,
    /// Their address, if Google verified it.
    pub verified_email: Option<String>,
    /// Their name, if they share it.
    pub name: Option<String>,
}

/// Why a token vouches for nobody. Logged as such; never with the token's claims.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    /// It is not three base64url parts of JSON within the limits.
    #[error("malformed token")]
    Malformed,
    /// Its header asks for an algorithm other than RS256.
    #[error("not RS256")]
    Algorithm,
    /// No key of the file has the id its header names.
    #[error("unknown key")]
    UnknownKey,
    /// The key file does not parse.
    #[error("unreadable key file")]
    Keys,
    /// The signature does not verify.
    #[error("bad signature")]
    Signature,
    /// Someone other than Google issued it.
    #[error("wrong issuer")]
    Issuer,
    /// It was issued to another site.
    #[error("wrong audience")]
    Audience,
    /// It expired, or claims to be issued in the future.
    #[error("expired")]
    Expired,
    /// It was not issued to this browser's attempt to sign in.
    #[error("wrong nonce")]
    Nonce,
}

#[derive(Deserialize)]
struct Header {
    alg: String,
    kid: Option<String>,
}

#[derive(Deserialize)]
struct Keys {
    keys: Vec<Key>,
}

#[derive(Deserialize)]
struct Key {
    kty: String,
    kid: Option<String>,
    n: Option<String>,
    e: Option<String>,
}

/// `true` and `"true"` alike, as Google has written `email_verified` both ways.
#[derive(Deserialize)]
#[serde(untagged)]
enum Flag {
    Bool(bool),
    Text(String),
}

#[derive(Deserialize)]
struct Claims {
    iss: String,
    aud: String,
    sub: String,
    exp: i64,
    iat: i64,
    nonce: Option<String>,
    email: Option<String>,
    email_verified: Option<Flag>,
    name: Option<String>,
}

/// The person `token` vouches for, if it verifies under `keys` (the JWKS file's text) for the
/// site whose client ID is `client_id`, at `now`, carrying `nonce`.
pub fn verify(
    token: &str,
    keys: &str,
    client_id: &str,
    nonce: &str,
    now: i64,
) -> Result<Person, Refusal> {
    if token.len() > TOKEN_LIMIT {
        return Err(Refusal::Malformed);
    }
    let mut parts = token.split('.');
    let (Some(head), Some(payload), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(Refusal::Malformed);
    };
    let header: Header = json(head)?;
    if header.alg != "RS256" {
        return Err(Refusal::Algorithm);
    }
    let key = key(keys, header.kid.as_deref())?;
    let signature = URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| Refusal::Malformed)?;
    let digest = Sha256::new()
        .chain_update(head)
        .chain_update(".")
        .chain_update(payload)
        .finalize();
    key.verify(Pkcs1v15Sign::new::<Sha256>(), &digest, &signature)
        .map_err(|_| Refusal::Signature)?;
    let claims: Claims = json(payload)?;
    if !ISSUERS.contains(&claims.iss.as_str()) {
        return Err(Refusal::Issuer);
    }
    if claims.aud != client_id {
        return Err(Refusal::Audience);
    }
    if claims.exp.saturating_add(SKEW) <= now || claims.iat.saturating_sub(SKEW) > now {
        return Err(Refusal::Expired);
    }
    if claims.nonce.as_deref() != Some(nonce) {
        return Err(Refusal::Nonce);
    }
    if claims.sub.is_empty() || claims.sub.len() > 255 {
        return Err(Refusal::Malformed);
    }
    let verified = matches!(claims.email_verified, Some(Flag::Bool(true)))
        || matches!(&claims.email_verified, Some(Flag::Text(text)) if text == "true");
    Ok(Person {
        subject: claims.sub,
        verified_email: claims.email.filter(|_| verified),
        name: claims.name,
    })
}

/// The JSON a base64url part encodes.
fn json<T: serde::de::DeserializeOwned>(part: &str) -> Result<T, Refusal> {
    let bytes = URL_SAFE_NO_PAD
        .decode(part)
        .map_err(|_| Refusal::Malformed)?;
    serde_json::from_slice(&bytes).map_err(|_| Refusal::Malformed)
}

/// The RSA key of `keys` whose id is `kid`.
fn key(keys: &str, kid: Option<&str>) -> Result<RsaPublicKey, Refusal> {
    let keys: Keys = serde_json::from_str(keys).map_err(|_| Refusal::Keys)?;
    let key = keys
        .keys
        .into_iter()
        .find(|key| key.kty == "RSA" && key.kid.is_some() && key.kid.as_deref() == kid)
        .ok_or(Refusal::UnknownKey)?;
    let number = |text: Option<String>| {
        let bytes = URL_SAFE_NO_PAD
            .decode(text.unwrap_or_default())
            .map_err(|_| Refusal::Keys)?;
        Ok(BigUint::from_bytes_be(&bytes))
    };
    let key = RsaPublicKey::new(number(key.n)?, number(key.e)?).map_err(|_| Refusal::Keys)?;
    if key.size() < 256 {
        return Err(Refusal::Keys);
    }
    Ok(key)
}

/// The nonce sent to Google for a browser that holds `secret` in its sign-in cookie: the
/// base64url SHA-256 of the secret, so the nonce, which travels in addresses, reveals nothing.
pub fn nonce_of(secret: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(secret.as_bytes()))
}

#[cfg(test)]
pub mod tests {
    //! Tokens signed by a key made in the test, which stands in for Google's.

    use std::sync::OnceLock;

    use rsa::RsaPrivateKey;
    use rsa::rand_core::OsRng;

    use super::*;

    /// The test key, made once per run.
    pub fn signer() -> &'static RsaPrivateKey {
        static KEY: OnceLock<RsaPrivateKey> = OnceLock::new();
        KEY.get_or_init(|| RsaPrivateKey::new(&mut OsRng, 2048).unwrap())
    }

    /// The JWKS of the test key, under the id `test`.
    pub fn keys() -> String {
        let public = signer().to_public_key();
        serde_json::json!({"keys": [{
            "kty": "RSA", "alg": "RS256", "use": "sig", "kid": "test",
            "n": URL_SAFE_NO_PAD.encode(public.n().to_bytes_be()),
            "e": URL_SAFE_NO_PAD.encode(public.e().to_bytes_be()),
        }]})
        .to_string()
    }

    /// `claims` signed under the test key, with `alg` in the header.
    pub fn sign(claims: &serde_json::Value, alg: &str) -> String {
        let header = serde_json::json!({"alg": alg, "kid": "test", "typ": "JWT"});
        let signed = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(header.to_string()),
            URL_SAFE_NO_PAD.encode(claims.to_string())
        );
        let digest = Sha256::digest(signed.as_bytes());
        let signature = signer()
            .sign(Pkcs1v15Sign::new::<Sha256>(), &digest)
            .unwrap();
        format!("{signed}.{}", URL_SAFE_NO_PAD.encode(signature))
    }

    /// The claims Google would send for `sub` signing in to `client` at 1000 with `nonce`.
    pub fn claims(sub: &str, client: &str, nonce: &str) -> serde_json::Value {
        serde_json::json!({
            "iss": "https://accounts.google.com", "aud": client, "sub": sub,
            "iat": 1000, "exp": 4600, "nonce": nonce,
            "email": format!("{sub}@example.com"), "email_verified": true, "name": "Ada Lovelace",
        })
    }

    #[test]
    fn a_good_token_vouches_for_its_person() {
        let token = sign(&claims("g1", "client", "n"), "RS256");
        let person = verify(&token, &keys(), "client", "n", 2000).unwrap();
        assert_eq!(person.subject, "g1");
        assert_eq!(person.verified_email.as_deref(), Some("g1@example.com"));
        assert_eq!(person.name.as_deref(), Some("Ada Lovelace"));
    }

    #[test]
    fn every_broken_rule_is_refused() {
        let keys = keys();
        let good = claims("g1", "client", "n");
        let with = |field: &str, value: serde_json::Value| {
            let mut claims = good.clone();
            claims[field] = value;
            sign(&claims, "RS256")
        };
        let cases = [
            (with("iss", "https://evil.example".into()), Refusal::Issuer),
            (with("aud", "other".into()), Refusal::Audience),
            (with("exp", 1500.into()), Refusal::Expired),
            (with("iat", 9000.into()), Refusal::Expired),
            (with("nonce", "m".into()), Refusal::Nonce),
            (sign(&good, "HS256"), Refusal::Algorithm),
            ("a.b".to_owned(), Refusal::Malformed),
            ("x".repeat(TOKEN_LIMIT + 1), Refusal::Malformed),
        ];
        for (token, refusal) in cases {
            assert_eq!(verify(&token, &keys, "client", "n", 2000), Err(refusal));
        }
        let good = sign(&good, "RS256");
        let (signed, _) = good.rsplit_once('.').unwrap();
        let forged = format!("{signed}.{}", URL_SAFE_NO_PAD.encode([7u8; 256]));
        assert_eq!(
            verify(&forged, &keys, "client", "n", 2000),
            Err(Refusal::Signature)
        );
        let tampered = good.replacen('.', ".e", 1);
        assert!(verify(&tampered, &keys, "client", "n", 2000).is_err());
        assert_eq!(
            verify(&good, r#"{"keys": []}"#, "client", "n", 2000),
            Err(Refusal::UnknownKey)
        );
    }

    #[test]
    fn an_unverified_address_is_not_vouched_for() {
        let mut claims = claims("g1", "client", "n");
        claims["email_verified"] = "false".into();
        let person = verify(&sign(&claims, "RS256"), &keys(), "client", "n", 2000).unwrap();
        assert_eq!(person.verified_email, None);
    }
}
