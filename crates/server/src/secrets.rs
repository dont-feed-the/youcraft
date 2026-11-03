//! Secrets: the random values that sign a browser or a program in, and their hashes.
//!
//! A secret is 32 random bytes written in base64url; the database keeps only its SHA-256, so
//! a copy of the database signs nobody in. Identifiers are 64 random bits.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use youcraft_store::credentials::SecretHash;

/// A new secret, and its hash.
pub fn new_secret() -> (String, SecretHash) {
    let secret = URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>());
    let hash = hash(&secret);
    (secret, hash)
}

/// The hash the database knows `secret` by.
pub fn hash(secret: &str) -> SecretHash {
    Sha256::digest(secret.as_bytes()).into()
}

/// 64 random bits, for a new identifier.
pub fn random_bits() -> u64 {
    rand::random()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_fresh_and_known_by_their_hash() {
        let (first, hash_of_first) = new_secret();
        let (second, _) = new_secret();
        assert_ne!(first, second);
        assert_eq!(first.len(), 43);
        assert_eq!(hash(&first), hash_of_first);
    }
}
