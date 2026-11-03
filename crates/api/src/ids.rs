//! Identifiers, written as people see them in addresses.
//!
//! Every identifier is 64 random bits written as 11 base64url symbols (RFC 4648 §5), most
//! significant first. Ten symbols carry 6 bits each and the eleventh the last 4 bits shifted left
//! by 2, so only 16 symbols may end an identifier and every value has exactly one spelling:
//! parsing rejects any other. The database stores the same 64 bits as an `i64`.

use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The 64 symbols of base64url, in value order.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// The length of a written identifier.
pub const LENGTH: usize = 11;

/// Write `value` as its 11 symbols.
pub fn encode(value: u64) -> String {
    let mut text = String::with_capacity(LENGTH);
    for index in 0..10 {
        let sextet = (value >> (58 - 6 * index)) & 0x3f;
        text.push(symbol(sextet));
    }
    text.push(symbol((value & 0xf) << 2));
    text
}

/// Read 11 symbols back into the value they spell, or `None` if they spell none.
pub fn decode(text: &str) -> Option<u64> {
    let bytes = text.as_bytes();
    if bytes.len() != LENGTH {
        return None;
    }
    let (last, head) = bytes.split_last()?;
    let mut value = 0u64;
    for &byte in head {
        value = (value << 6) | value_of(byte)?;
    }
    let tail = value_of(*last)?;
    if tail & 0b11 != 0 {
        return None;
    }
    Some((value << 4) | (tail >> 2))
}

fn symbol(sextet: u64) -> char {
    ALPHABET
        .get(sextet as usize)
        .map_or('A', |&byte| char::from(byte))
}

fn value_of(byte: u8) -> Option<u64> {
    let value = match byte {
        b'A'..=b'Z' => byte - b'A',
        b'a'..=b'z' => byte - b'a' + 26,
        b'0'..=b'9' => byte - b'0' + 52,
        b'-' => 62,
        b'_' => 63,
        _ => return None,
    };
    Some(u64::from(value))
}

/// Why a written identifier was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("not an identifier: 11 base64url symbols expected")]
pub struct InvalidId;

macro_rules! identifier {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u64);

        impl $name {
            /// The identifier made of these 64 bits.
            pub const fn from_bits(bits: u64) -> Self {
                Self(bits)
            }

            /// Its 64 bits.
            pub const fn bits(self) -> u64 {
                self.0
            }

            /// The same 64 bits as the database stores them.
            pub const fn to_i64(self) -> i64 {
                self.0 as i64
            }

            /// The identifier the database stored as `value`.
            pub const fn from_i64(value: i64) -> Self {
                Self(value as u64)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&encode(self.0))
            }
        }

        impl FromStr for $name {
            type Err = InvalidId;

            fn from_str(text: &str) -> Result<Self, InvalidId> {
                decode(text).map(Self).ok_or(InvalidId)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&encode(self.0))
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
                text.parse().map_err(serde::de::Error::custom)
            }
        }

        impl JsonSchema for $name {
            fn schema_name() -> std::borrow::Cow<'static, str> {
                stringify!($name).into()
            }

            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({
                    "type": "string",
                    "pattern": "^[A-Za-z0-9_-]{10}[AEIMQUYcgkosw048]$"
                })
            }
        }
    };
}

identifier!(
    /// A video, as in `/watch?v=…`.
    VideoId
);
identifier!(
    /// A channel; people reach it by its handle, `/@…`.
    ChannelId
);
identifier!(
    /// A playlist made by someone, written `PL` + its identifier in `/playlist?list=…`.
    PlaylistId
);
identifier!(
    /// A comment or a reply.
    CommentId
);
identifier!(
    /// A person's account, never shown to others.
    AccountId
);
identifier!(
    /// A personal token, as its owner lists and revokes it; never its secret.
    TokenId
);

/// A list of videos, as `/playlist?list=…` names it: `WL` the viewer's Watch later, `LL` the
/// videos they liked, `PL` and its identifier a playlist someone made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum List {
    /// The viewer's Watch later list.
    WatchLater,
    /// The videos the viewer liked.
    Liked,
    /// A playlist someone made.
    Playlist(PlaylistId),
}

impl fmt::Display for List {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            List::WatchLater => f.write_str("WL"),
            List::Liked => f.write_str("LL"),
            List::Playlist(id) => write!(f, "PL{id}"),
        }
    }
}

impl FromStr for List {
    type Err = InvalidId;

    fn from_str(text: &str) -> Result<List, InvalidId> {
        match text {
            "WL" => Ok(List::WatchLater),
            "LL" => Ok(List::Liked),
            _ => text
                .strip_prefix("PL")
                .ok_or(InvalidId)?
                .parse()
                .map(List::Playlist),
        }
    }
}

impl Serialize for List {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for List {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for List {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "List".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^(WL|LL|PL[A-Za-z0-9_-]{10}[AEIMQUYcgkosw048])$"
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_value_has_one_spelling() {
        for value in [
            0,
            1,
            15,
            16,
            0x0123_4567_89ab_cdef,
            u64::MAX,
            u64::MAX - 1,
            1 << 63,
        ] {
            let text = encode(value);
            assert_eq!(text.len(), LENGTH);
            assert_eq!(decode(&text), Some(value), "{text}");
        }
        assert_eq!(encode(0), "AAAAAAAAAAA");
        assert_eq!(encode(u64::MAX), "__________8");
    }

    #[test]
    fn other_spellings_are_refused() {
        assert_eq!(
            decode("AAAAAAAAAAB"),
            None,
            "the last symbol's low bits must be zero"
        );
        assert_eq!(decode("AAAAAAAAAA"), None);
        assert_eq!(decode("AAAAAAAAAAAA"), None);
        assert_eq!(decode("AAAAAAAAAA="), None);
        assert_eq!(decode("AAAAAAAAAAé"), None);
        assert_eq!(decode(""), None);
    }

    #[test]
    fn the_database_keeps_all_64_bits() {
        for bits in [0, 1, u64::MAX, 1 << 63] {
            let id = VideoId::from_bits(bits);
            assert_eq!(VideoId::from_i64(id.to_i64()), id);
        }
    }

    #[test]
    fn identifiers_travel_as_their_spelling() {
        let id = VideoId::from_bits(0x0123_4567_89ab_cdef);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{id}\""));
        assert_eq!(serde_json::from_str::<VideoId>(&json).unwrap(), id);
        assert!(serde_json::from_str::<VideoId>("\"AAAAAAAAAAB\"").is_err());
    }

    #[test]
    fn lists_are_written_as_youtube_writes_them() {
        let made = List::Playlist(PlaylistId::from_bits(0));
        for (list, text) in [
            (List::WatchLater, "WL"),
            (List::Liked, "LL"),
            (made, "PLAAAAAAAAAAA"),
        ] {
            assert_eq!(list.to_string(), text);
            assert_eq!(text.parse::<List>(), Ok(list));
        }
        for wrong in ["", "PL", "XX", "AAAAAAAAAAA", "PLAAAAAAAAAAB", "wl"] {
            assert!(wrong.parse::<List>().is_err(), "{wrong}");
        }
    }
}
