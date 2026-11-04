//! Personal tokens: made, listed and revoked by their owner alone.

use youcraft_api::commands::site::Nothing;
use youcraft_api::commands::token::{
    Create, Created, List, NAME_LIMIT, Naming, Revoke, TOKEN_LIMIT, Token, Which,
};
use youcraft_api::{ApiError, TokenId};
use youcraft_store::StoreError;

use crate::dispatch::Handle;
use crate::handlers::input::line;
use crate::secrets::{new_secret, random_bits};
use crate::site::{Caller, Site, now};

/// What a token's secret starts with, so that people and scanners recognize one.
const PREFIX: &str = "yct_";

impl Handle<Create> for Site {
    async fn handle(&self, caller: Caller, naming: Naming) -> Result<Created, ApiError> {
        let account = caller.signed_in()?;
        let name = line("name", &naming.name, NAME_LIMIT)?;
        let (secret, _) = new_secret();
        let secret = format!("{PREFIX}{secret}");
        let hash = crate::secrets::hash(&secret);
        let id = TokenId::from_bits(random_bits());
        let (now, channel) = (now(), caller.channel);
        let made = self
            .db(move |store| {
                let made =
                    store.create_token((id, &hash), (account, channel), &name, now, TOKEN_LIMIT)?;
                if !made {
                    return Ok(None);
                }
                let listed = store
                    .tokens(account)?
                    .into_iter()
                    .find(|token| token.id == id);
                listed
                    .map(Some)
                    .ok_or_else(|| StoreError::Corrupt(format!("token {id} vanished")))
            })
            .await?;
        let token = made.ok_or(ApiError::TooLarge {
            limit: TOKEN_LIMIT as u64,
        })?;
        Ok(Created { token, secret })
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
