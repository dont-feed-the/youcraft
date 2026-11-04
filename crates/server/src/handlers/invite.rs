//! Invitations, made and withdrawn by administrators.

use youcraft_api::commands::invite::{
    Address, Create, EMAIL_LIMIT, Invitation, Invited, List, NewInvitation, Revoke,
};
use youcraft_api::commands::site::Nothing;
use youcraft_api::{ApiError, Rule};

use crate::dispatch::Handle;
use crate::site::{Caller, Site, now};

/// `email` trimmed, if it looks like an address: one `@` between a local part and a domain,
/// no spaces or control characters, at most [`EMAIL_LIMIT`] bytes.
pub(super) fn address(email: &str) -> Result<String, ApiError> {
    let email = email.trim();
    let well_formed = email.len() <= EMAIL_LIMIT
        && !email.chars().any(|c| c.is_whitespace() || c.is_control())
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.contains('@')
        });
    if well_formed {
        Ok(email.to_owned())
    } else {
        Err(ApiError::invalid("email", Rule::Malformed))
    }
}

impl Handle<Create> for Site {
    async fn handle(&self, _: Caller, invitation: NewInvitation) -> Result<Invited, ApiError> {
        // An invitation raises an account above what every account has; below or at it, it
        // would mean nothing.
        if invitation.role <= self.config().signed_in_role {
            return Err(ApiError::invalid("role", Rule::NotOffered));
        }
        let email = address(&invitation.email)?;
        let now = now();
        self.db(move |store| store.invite(&email, invitation.role, now))
            .await
    }
}

impl Handle<List> for Site {
    async fn handle(&self, _: Caller, _: Nothing) -> Result<Vec<Invitation>, ApiError> {
        self.db(|store| store.invitations()).await
    }
}

impl Handle<Revoke> for Site {
    async fn handle(&self, _: Caller, which: Address) -> Result<Nothing, ApiError> {
        let email = address(&which.email)?;
        let revoked = self
            .db(move |store| store.revoke_invitation(&email))
            .await?;
        revoked.then_some(Nothing {}).ok_or(ApiError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_addresses_are_invited() {
        assert_eq!(address(" ada@example.com ").unwrap(), "ada@example.com");
        for wrong in [
            "ada",
            "@example.com",
            "ada@example",
            "a b@example.com",
            "a@b@c.com",
        ] {
            assert!(address(wrong).is_err(), "{wrong}");
        }
        assert!(address(&format!("{}@example.com", "a".repeat(250))).is_err());
    }
}
