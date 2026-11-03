//! Who may run a command: roles, from anyone at all to the site's administrators.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What a caller is to the site. Each role can do everything the roles before it can.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Anyone, signed in or not.
    Anyone,
    /// A person signed in with Google: likes, subscribes, comments, keeps playlists.
    Viewer,
    /// An invited person who also publishes videos.
    Creator,
    /// Someone who runs the site: invites, moderates, blocks.
    Admin,
}

impl Role {
    /// Whether a caller with this role may run a command that requires `required`.
    pub fn allows(self, required: Role) -> bool {
        self >= required
    }

    /// The name the database and the command line use.
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Anyone => "anyone",
            Role::Viewer => "viewer",
            Role::Creator => "creator",
            Role::Admin => "admin",
        }
    }

    /// The role named `name`, as [`Role::as_str`] writes it.
    pub fn from_name(name: &str) -> Option<Role> {
        [Role::Anyone, Role::Viewer, Role::Creator, Role::Admin]
            .into_iter()
            .find(|role| role.as_str() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_are_cumulative() {
        assert!(Role::Admin.allows(Role::Creator));
        assert!(Role::Creator.allows(Role::Viewer));
        assert!(Role::Viewer.allows(Role::Anyone));
        assert!(!Role::Anyone.allows(Role::Viewer));
        assert!(!Role::Viewer.allows(Role::Creator));
    }

    #[test]
    fn names_round_trip() {
        for role in [Role::Anyone, Role::Viewer, Role::Creator, Role::Admin] {
            assert_eq!(Role::from_name(role.as_str()), Some(role));
        }
        assert_eq!(Role::from_name("owner"), None);
    }
}
