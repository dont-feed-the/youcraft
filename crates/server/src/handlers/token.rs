//! Personal tokens: made, listed and revoked by their owner alone.

use youcraft_api::commands::site::Nothing;
use youcraft_api::commands::token::{
    Create, Created, List, NAME_LIMIT, Naming, Revoke, TOKEN_LIMIT, Token, Which,
};
use youcraft_api::{ApiError, TokenId};

use crate::dispatch::Handle;
use crate::secrets::{new_secret, random_bits};
use crate::site::{Caller, Site, now};

/// What a token's secret starts with, so that people and scanners recognize one.
const PREFIX: &str = "yct_";

impl Handle<Create> for Site {
    async fn handle(&self, caller: Caller, naming: Naming) -> Result<Created, ApiError> {
        let account = caller.signed_in()?;
        let name = naming.name.trim().to_owned();
        let length = name.chars().count();
        if length == 0 || length > NAME_LIMIT || name.chars().any(char::is_control) {
            return Err(ApiError::invalid(
                "name",
                "1 to 100 characters, no control ones",
            ));
        }
        let (secret, _) = new_secret();
        let secret = format!("{PREFIX}{secret}");
        let hash = crate::secrets::hash(&secret);
        let id = TokenId::from_bits(random_bits());
        let (now, stored) = (now(), name.clone());
        let made = self
            .db(move |store| store.create_token((id, &hash), account, &stored, now, TOKEN_LIMIT))
            .await?;
        if !made {
            return Err(ApiError::TooLarge {
                limit: TOKEN_LIMIT as u64,
            });
        }
        Ok(Created {
            token: Token {
                id,
                name,
                created_at: now,
                last_used_at: None,
            },
            secret,
        })
    }
}

impl Handle<List> for Site {
    async fn handle(&self, caller: Caller, _: Nothing) -> Result<Vec<Token>, ApiError> {
        let account = caller.signed_in()?;
        self.db(move |store| store.tokens(account)).await
    }
}

impl Handle<Revoke> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<Nothing, ApiError> {
        let account = caller.signed_in()?;
        let revoked = self
            .db(move |store| store.revoke_token(account, which.token))
            .await?;
        revoked.then_some(Nothing {}).ok_or(ApiError::NotFound)
    }
}
