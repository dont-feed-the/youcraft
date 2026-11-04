//! Who may run a command: the levels a caller acts at, from anyone at all to the site's
//! administrators, and the roles an account holds.
//!
//! Every command requires a [`Role`]. An account holds a [`Grant`], which the site may raise to
//! what it grants every account; it acts with that role while it is not blocked, and as
//! [`Role::SignedIn`] while it is, managing only what is its own. A visitor who is not signed in
//! acts as [`Role::Anyone`].

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What a caller may do. Each role can do everything the roles before it can.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Anyone, signed in or not: looks at what is public.
    Anyone,
    /// Anyone signed in, blocked or not: also reads what is their own and takes it back (their
    /// history, tokens, preferences, lists, ratings and subscriptions, what they wrote or
    /// published).
    SignedIn,
    /// A signed-in account that is not blocked: also acts toward others: likes, subscribes,
    /// comments, keeps playlists.
    Viewer,
    /// Also publishes videos.
    Creator,
    /// Also runs the site: invites, moderates, blocks.
    Admin,
}

impl Role {
    /// Every role, the least first.
    pub const ALL: [Role; 5] = [
        Role::Anyone,
        Role::SignedIn,
        Role::Viewer,
        Role::Creator,
        Role::Admin,
    ];

    /// Whether a caller with this role may run a command that requires `required`.
    pub fn allows(self, required: Role) -> bool {
        self >= required
    }

    /// The name the command line shows, as JSON writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Anyone => "anyone",
            Role::SignedIn => "signed_in",
            Role::Viewer => "viewer",
            Role::Creator => "creator",
            Role::Admin => "admin",
        }
    }
}

/// A role an account holds, as an administrator, an invitation or the site's policy grants it:
/// the roles of [`Role`] a person can be given, never one that only describes a caller.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Grant {
    /// Watches, likes, subscribes, comments, keeps playlists.
    Viewer,
    /// Also publishes videos.
    Creator,
    /// Also runs the site.
    Admin,
}

impl Grant {
    /// Every grant, the least first.
    pub const ALL: [Grant; 3] = [Grant::Viewer, Grant::Creator, Grant::Admin];

    /// The name the database and the command line use, as JSON writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Grant::Viewer => "viewer",
            Grant::Creator => "creator",
            Grant::Admin => "admin",
        }
    }

    /// The grant named `name`, as [`Grant::as_str`] writes it.
    pub fn from_name(name: &str) -> Option<Grant> {
        Grant::ALL.into_iter().find(|grant| grant.as_str() == name)
    }
}

impl From<Grant> for Role {
    fn from(grant: Grant) -> Role {
        match grant {
            Grant::Viewer => Role::Viewer,
            Grant::Creator => Role::Creator,
            Grant::Admin => Role::Admin,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_are_cumulative() {
        for (i, role) in Role::ALL.iter().enumerate() {
            for (j, required) in Role::ALL.iter().enumerate() {
                assert_eq!(role.allows(*required), i >= j, "{role:?} for {required:?}");
            }
            assert_eq!(serde_json::to_value(role).unwrap(), role.as_str());
        }
    }

    #[test]
    fn grants_keep_their_order_and_names() {
        for pair in Grant::ALL.windows(2) {
            assert!(pair[0] < pair[1] && Role::from(pair[0]) < Role::from(pair[1]));
        }
        for grant in Grant::ALL {
            assert_eq!(Grant::from_name(grant.as_str()), Some(grant));
            let json = serde_json::to_value(grant).unwrap();
            assert_eq!(json, serde_json::to_value(Role::from(grant)).unwrap());
            assert_eq!(json, grant.as_str());
        }
        assert_eq!(Grant::from_name("anyone"), None);
        assert_eq!(Grant::from_name("signed_in"), None);
    }
}
