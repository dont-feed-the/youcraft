//! Why a command failed, in a form the caller can act on.
//!
//! An error carries no prose: the page translates its kind (and, for a field, the [`Rule`] it
//! breaks) into the reader's language, and the command line prints it in English. The one
//! exception is [`Rule::Schema`], the JSON decoder's account of input that does not match a
//! command's schema, which only a program sends and only its programmer reads. The server maps
//! each kind to its HTTP status.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Why a command did not run, or did not finish.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, thiserror::Error)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum ApiError {
    /// The command needs a signed-in caller.
    #[error("sign in first")]
    SignInRequired,
    /// The caller's role does not allow it, or the caller is blocked.
    #[error("not allowed")]
    Forbidden,
    /// No such video, channel, comment or playlist, or the caller may not see it.
    #[error("not found")]
    NotFound,
    /// No command has this id.
    #[error("no such command")]
    UnknownCommand,
    /// The input does not parse, or a field breaks its rule.
    #[error("invalid {field}: {rule}")]
    Invalid {
        /// The field, as the input names it; `input` for the input as a whole.
        field: String,
        /// The rule it breaks.
        #[serde(flatten)]
        rule: Rule,
    },
    /// The input is larger than the site accepts.
    #[error("too large: at most {limit}")]
    TooLarge {
        /// The largest size accepted, in the field's own unit.
        limit: u64,
    },
    /// The action conflicts with the current state (a handle already taken, say).
    #[error("conflict")]
    Conflict,
    /// The caller changed the site too often of late; it may try again after `retry_after`.
    #[error("too often: try again in {retry_after} s")]
    TooOften {
        /// Seconds until the caller may write again.
        retry_after: u64,
    },
    /// The server failed; the details are in its log.
    #[error("server error")]
    Internal,
}

/// The rule a field of a command's input breaks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, thiserror::Error)]
#[serde(tag = "rule", rename_all = "snake_case")]
pub enum Rule {
    /// It must hold something: text that is not blank, a file with bytes.
    #[error("required")]
    Required,
    /// It holds more than `limit` characters, counted as Unicode scalar values (what JSON
    /// Schema's `maxLength` counts), after its ends are trimmed.
    #[error("at most {limit} characters")]
    TooLong {
        /// The most characters it may hold.
        limit: u64,
    },
    /// It must fit on one line.
    #[error("on one line")]
    OneLine,
    /// It holds a character that text on the site may not: a control character, or one that
    /// reorders the text around it (a bidirectional embedding, override or isolate).
    #[error("no control or text-reordering characters")]
    Characters,
    /// It is not in the form the field takes: an e-mail address, a cursor, a byte offset.
    #[error("not in the form it takes")]
    Malformed,
    /// It names something the site does not offer here: a language, a role, a list that
    /// cannot change.
    #[error("not one the site offers here")]
    NotOffered,
    /// The input does not match the command's input schema.
    #[error("{mismatch}")]
    Schema {
        /// How it does not, as the JSON decoder says it, in English.
        mismatch: String,
    },
}

impl ApiError {
    /// An [`ApiError::Invalid`] for `field`, which breaks `rule`.
    pub fn invalid(field: &str, rule: Rule) -> ApiError {
        ApiError::Invalid {
            field: field.to_owned(),
            rule,
        }
    }

    /// An [`ApiError::Invalid`] for input that does not decode as the command's, for `why`.
    pub fn schema(why: &impl std::fmt::Display) -> ApiError {
        ApiError::invalid(
            "input",
            Rule::Schema {
                mismatch: why.to_string(),
            },
        )
    }

    /// The HTTP status this error is answered with.
    pub fn status(&self) -> u16 {
        match self {
            ApiError::SignInRequired => 401,
            ApiError::Forbidden => 403,
            ApiError::NotFound | ApiError::UnknownCommand => 404,
            ApiError::Invalid { .. } => 422,
            ApiError::TooLarge { .. } => 413,
            ApiError::Conflict => 409,
            ApiError::TooOften { .. } => 429,
            ApiError::Internal => 500,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_broken_rule_travels_beside_its_field() {
        let error = ApiError::invalid("title", Rule::TooLong { limit: 100 });
        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"error": "invalid", "field": "title", "rule": "too_long", "limit": 100})
        );
        assert_eq!(serde_json::from_value::<ApiError>(json).unwrap(), error);
        assert_eq!(error.to_string(), "invalid title: at most 100 characters");
        let schema = ApiError::schema(&"missing field `video`");
        let json = serde_json::to_value(&schema).unwrap();
        assert_eq!(serde_json::from_value::<ApiError>(json).unwrap(), schema);
        assert_eq!(schema.to_string(), "invalid input: missing field `video`");
    }
}
