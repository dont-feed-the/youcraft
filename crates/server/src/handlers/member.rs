//! The members of channels: owners list them, invite addresses, change roles and take members
//! off; any member leaves; a person answers the invitations of their verified address.
//!
//! Every change keeps the channel with an owner: the store answers [`Change::LastOwner`], which
//! is a conflict with what is there.

use youcraft_api::commands::account::Membership;
use youcraft_api::commands::channel::Which;
use youcraft_api::commands::member::{
    Accept, Decline, INVITATION_LIMIT, INVITATION_SECONDS, Invitation, Invite, Invitee, List,
    MEMBER_LIMIT, Members, NewInvitation, NewRole, Offer, Offers, Remove, Seat, SetRole, Uninvite,
};
use youcraft_api::commands::site::Nothing;
use youcraft_api::{ApiError, ChannelRole};
use youcraft_store::members::{Change, Invited, Joined};

use crate::dispatch::Handle;
use crate::handlers::invite::address;
use crate::site::{Caller, Site, now};

/// What changing a member did.
fn changed(change: Change) -> Result<Nothing, ApiError> {
    match change {
        Change::Done => Ok(Nothing {}),
        Change::NotMember => Err(ApiError::NotFound),
        Change::LastOwner => Err(ApiError::Conflict),
    }
}

impl Handle<List> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<Members, ApiError> {
        self.allow_in_channel(caller, which.channel, ChannelRole::Owner)
            .await?;
        let now = now();
        self.db(move |store| store.members(which.channel, now))
            .await
    }
}

impl Handle<Invite> for Site {
    async fn handle(&self, caller: Caller, new: NewInvitation) -> Result<Invitation, ApiError> {
        self.allow_in_channel(caller, new.channel, ChannelRole::Owner)
            .await?;
        let email = address(&new.email)?;
        let now = now();
        let invited = self
            .db(move |store| {
                store.invite_member(
                    new.channel,
                    (&email, new.role),
                    (now, now.saturating_add(INVITATION_SECONDS)),
                    INVITATION_LIMIT,
                )
            })
            .await?;
        match invited {
            Invited::Done(invitation) => Ok(invitation),
            Invited::Member => Err(ApiError::Conflict),
            Invited::Full => Err(ApiError::TooLarge {
                limit: INVITATION_LIMIT as u64,
            }),
        }
    }
}

impl Handle<Uninvite> for Site {
    async fn handle(&self, caller: Caller, invitee: Invitee) -> Result<Nothing, ApiError> {
        self.allow_in_channel(caller, invitee.channel, ChannelRole::Owner)
            .await?;
        let email = invitee.email.trim().to_owned();
        let withdrawn = self
            .db(move |store| store.uninvite(invitee.channel, &email))
            .await?;
        withdrawn.then_some(Nothing {}).ok_or(ApiError::NotFound)
    }
}

impl Handle<SetRole> for Site {
    async fn handle(&self, caller: Caller, new: NewRole) -> Result<Nothing, ApiError> {
        self.allow_in_channel(caller, new.channel, ChannelRole::Owner)
            .await?;
        let change = self
            .db(move |store| store.set_member_role(new.channel, new.account, new.role))
            .await?;
        changed(change)
    }
}

impl Handle<Remove> for Site {
    async fn handle(&self, caller: Caller, seat: Seat) -> Result<Nothing, ApiError> {
        // Anyone leaves; only owners take others off.
        if caller.account != Some(seat.account) {
            self.allow_in_channel(caller, seat.channel, ChannelRole::Owner)
                .await?;
        }
        let change = self
            .db(move |store| store.remove_member(seat.channel, seat.account))
            .await?;
        changed(change)
    }
}

impl Handle<Offers> for Site {
    async fn handle(&self, caller: Caller, _: Nothing) -> Result<Vec<Offer>, ApiError> {
        let account = caller.signed_in()?;
        let now = now();
        self.db(move |store| store.offers(account, now)).await
    }
}

impl Handle<Accept> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<Membership, ApiError> {
        let account = caller.signed_in()?;
        let (now, held) = (now(), self.config().channels_per_account);
        let joined = self
            .db(move |store| store.accept(account, which.channel, now, (held, MEMBER_LIMIT)))
            .await?;
        match joined {
            Joined::Done(membership) => Ok(membership),
            Joined::Uninvited => Err(ApiError::NotFound),
            Joined::Full => Err(ApiError::TooLarge {
                limit: u64::try_from(held.min(MEMBER_LIMIT)).unwrap_or(u64::MAX),
            }),
        }
    }
}

impl Handle<Decline> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<Nothing, ApiError> {
        let account = caller.signed_in()?;
        let declined = self
            .db(move |store| store.decline(account, which.channel))
            .await?;
        declined.then_some(Nothing {}).ok_or(ApiError::NotFound)
    }
}
