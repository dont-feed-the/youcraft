//! Why a command failed, in a form the caller can act on.
//!
//! An error carries no prose: the page translates its kind into the reader's language, and the
//! command line prints it. The server maps each kind to its HTTP status.

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
        /// The field, as the input names it.
        field: String,
        /// The rule it breaks, in English, for the command line and logs.
        rule: String,
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
    /// The server failed; the details are in its log.
    #[error("server error")]
    Internal,
}

impl ApiError {
    /// An [`ApiError::Invalid`] for `field`, which breaks `rule`.
    pub fn invalid(field: &str, rule: &str) -> ApiError {
        ApiError::Invalid {
            field: field.to_owned(),
            rule: rule.to_owned(),
        }
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
            ApiError::Internal => 500,
        }
    }
}
