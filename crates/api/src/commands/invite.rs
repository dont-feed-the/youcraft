//! Invitations: the e-mail addresses an administrator lets publish or run the site.
//!
//! An invitation waits until the person with that address signs in with Google (which vouches
//! for the address), then raises their role and is spent. Inviting someone who already has an
//! account raises their role at once.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::commands::site::Nothing;
use crate::records::ChannelCard;
use crate::role::{Grant, Role};

/// An invitation waiting for its person to sign in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Invitation {
    /// The address invited, as Google reports it.
    pub email: String,
    /// The role it gives: above what the site grants every account.
    pub role: Grant,
    /// When it was made, in seconds since 1970.
    pub created_at: i64,
}

/// Whom to invite, as what.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewInvitation {
    /// Their e-mail address.
    pub email: String,
    /// The role it gives: above what the site grants every account.
    pub role: Grant,
}

/// What an invitation did.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Invited {
    /// Nobody has this address yet: the invitation waits for them.
    Waiting {
        /// The invitation.
        invitation: Invitation,
    },
    /// The person already had an account, whose role is now at least the one invited.
    Applied {
        /// Their channel.
        channel: ChannelCard,
    },
}

/// One invitation, by its address.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Address {
    /// The address invited.
    pub email: String,
}

/// The most bytes an e-mail address has (RFC 5321).
pub const EMAIL_LIMIT: usize = 254;

/// Invite someone.
pub struct Create;

impl Command for Create {
    const ID: &'static str = "invite.create";
    const REQUIRES: Role = Role::Admin;
    const SUMMARY: &'static str =
        "Invite an e-mail address to publish (creator) or to run the site (admin).";
    type Input = NewInvitation;
    type Output = Invited;
}

/// List the invitations still waiting.
pub struct List;

impl Command for List {
    const ID: &'static str = "invite.list";
    const REQUIRES: Role = Role::Admin;
    const SUMMARY: &'static str = "List the invitations whose person has not signed in yet.";
    type Input = Nothing;
    type Output = Vec<Invitation>;
}

/// Withdraw an invitation.
pub struct Revoke;

impl Command for Revoke {
    const ID: &'static str = "invite.revoke";
    const REQUIRES: Role = Role::Admin;
    const SUMMARY: &'static str = "Withdraw an invitation whose person has not signed in yet.";
    type Input = Address;
    type Output = Nothing;
}
