//! Commands about the people who run a channel: its members, each with a [`ChannelRole`], and
//! the invitations its owners send.
//!
//! An owner invites an e-mail address as a role; the person with that address, once Google
//! vouches for it, sees the invitation and says yes or no within [`INVITATION_SECONDS`]. Owners
//! change members' roles and take them off the channel; any member leaves it. A channel always
//! keeps an owner: its last owner neither leaves nor becomes less, and deletes the channel
//! instead (`channel.delete`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::commands::account::Membership;
use crate::commands::channel::Which;
use crate::commands::invite::EMAIL_LIMIT;
use crate::commands::site::Nothing;
use crate::ids::{AccountId, ChannelId};
use crate::records::ChannelCard;
use crate::role::{ChannelRole, Role};

/// The most members a channel has.
pub const MEMBER_LIMIT: usize = 100;
/// The most invitations a channel has waiting.
pub const INVITATION_LIMIT: usize = 50;
/// How long an invitation waits for its answer: a week.
pub const INVITATION_SECONDS: i64 = 7 * 24 * 60 * 60;

/// A member of a channel, as its owners see them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Member {
    /// Their account.
    pub account: AccountId,
    /// Their name, as Google gave it when they first signed in.
    pub name: String,
    /// The address they were invited at, which Google vouched for; empty for one it did not.
    pub email: String,
    /// Their role.
    pub role: ChannelRole,
    /// When they joined, in seconds since 1970.
    pub since: i64,
}

/// An invitation a channel sent, waiting for its answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Invitation {
    /// The address invited.
    pub email: String,
    /// The role it offers.
    pub role: ChannelRole,
    /// When it lapses unanswered, in seconds since 1970.
    pub expires_at: i64,
}

/// Who runs a channel: its members, the oldest first, and the invitations it is waiting on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Members {
    /// The members.
    pub members: Vec<Member>,
    /// The invitations waiting, the newest first.
    pub invitations: Vec<Invitation>,
}

/// An invitation the caller received, waiting for their answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Offer {
    /// The channel that invites them.
    pub channel: ChannelCard,
    /// The role it offers.
    pub role: ChannelRole,
    /// When it lapses unanswered, in seconds since 1970.
    pub expires_at: i64,
}

/// Whom to invite to a channel, as what.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewInvitation {
    /// The channel, which the caller owns.
    pub channel: ChannelId,
    /// The address of the person invited, at most [`EMAIL_LIMIT`] bytes.
    #[schemars(length(min = 1, max = EMAIL_LIMIT))]
    pub email: String,
    /// The role it offers.
    pub role: ChannelRole,
}

/// One invitation of a channel, by its address.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Invitee {
    /// The channel, which the caller owns.
    pub channel: ChannelId,
    /// The address invited.
    pub email: String,
}

/// A member of a channel: their seat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Seat {
    /// The channel.
    pub channel: ChannelId,
    /// The member's account.
    pub account: AccountId,
}

/// A new role for a member of a channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewRole {
    /// The channel, which the caller owns.
    pub channel: ChannelId,
    /// The member's account.
    pub account: AccountId,
    /// Their role from now on.
    pub role: ChannelRole,
}

/// Who runs a channel.
pub struct List;

impl Command for List {
    const ID: &'static str = "member.list";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str =
        "List the members of a channel you own, with their roles, and the invitations it sent.";
    type Input = Which;
    type Output = Members;
    const WRITES: bool = false;
}

/// Invite an address to a channel.
pub struct Invite;

impl Command for Invite {
    const ID: &'static str = "member.invite";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str = "Invite an e-mail address to a channel you own, as a viewer, an editor, a manager or an owner; the person says yes or no within a week.";
    type Input = NewInvitation;
    type Output = Invitation;
    const WRITES: bool = true;
}

/// Withdraw an invitation of a channel.
pub struct Uninvite;

impl Command for Uninvite {
    const ID: &'static str = "member.uninvite";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str =
        "Withdraw an invitation of a channel you own before its person answers.";
    type Input = Invitee;
    type Output = Nothing;
    const WRITES: bool = true;
}

/// Change a member's role.
pub struct SetRole;

impl Command for SetRole {
    const ID: &'static str = "member.set_role";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str =
        "Change the role of a member of a channel you own; the channel keeps an owner.";
    type Input = NewRole;
    type Output = Nothing;
    const WRITES: bool = true;
}

/// Take a member off a channel, or leave it.
pub struct Remove;

impl Command for Remove {
    const ID: &'static str = "member.remove";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str = "Take a member off a channel you own, or leave a channel yourself; the channel keeps an owner.";
    type Input = Seat;
    type Output = Nothing;
    const WRITES: bool = true;
}

/// The invitations the caller received.
pub struct Offers;

impl Command for Offers {
    const ID: &'static str = "member.offers";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str =
        "List the invitations to run a channel that wait for your answer.";
    type Input = Nothing;
    type Output = Vec<Offer>;
    const WRITES: bool = false;
}

/// Say yes to an invitation.
pub struct Accept;

impl Command for Accept {
    const ID: &'static str = "member.accept";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str = "Join a channel that invited you, in the role it offered.";
    type Input = Which;
    type Output = Membership;
    const WRITES: bool = true;
}

/// Say no to an invitation.
pub struct Decline;

impl Command for Decline {
    const ID: &'static str = "member.decline";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str = "Turn down the invitation of a channel.";
    type Input = Which;
    type Output = Nothing;
    const WRITES: bool = true;
}
