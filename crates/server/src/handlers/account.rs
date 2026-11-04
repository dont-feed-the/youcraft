//! Commands about the signed-in person's own account, and the channel each of their credentials
//! acts as.

use youcraft_api::commands::account::{
    ActAs, Acting, Get, Me, Preferences, SetPreferences, SignOut,
};
use youcraft_api::commands::site::Nothing;
use youcraft_api::{AccountId, ApiError, ChannelId, ChannelRole, Rule};

use crate::dispatch::Handle;
use crate::site::{Caller, Credential, Site, now};

impl Site {
    /// The person `account`, acting as `channel`, as they see themselves.
    pub async fn me(
        &self,
        account: AccountId,
        channel: Option<ChannelId>,
    ) -> Result<Option<Me>, ApiError> {
        let now = now();
        let me = self
            .db(move |store| store.me(account, channel, now))
            .await?;
        // The site's floor raises the role of every account that is not blocked.
        let floor = self.config().signed_in_role;
        Ok(me.map(|me| Me {
            role: if me.blocked {
                me.role
            } else {
                me.role.max(floor)
            },
            ..me
        }))
    }

    /// Have the credential of `caller` act as `channel` from now on, which they must manage or
    /// own; administrators act only as their own channels too.
    pub async fn act_as(&self, caller: Caller, channel: ChannelId) -> Result<(), ApiError> {
        let account = caller.signed_in()?;
        let role = self
            .db(move |store| store.member_role(channel, account))
            .await?;
        if !role.is_some_and(|role| role.allows(ChannelRole::Manager)) {
            return Err(ApiError::Forbidden);
        }
        match caller.credential {
            Credential::Session(hash) => {
                self.db(move |store| store.act_as_in_session(&hash, channel))
                    .await
            }
            Credential::Token(hash) => {
                self.db(move |store| store.act_as_with_token(&hash, channel))
                    .await
            }
            Credential::Nothing | Credential::Operator => Err(ApiError::Forbidden),
        }
    }
}

impl Handle<Get> for Site {
    async fn handle(&self, caller: Caller, _: Nothing) -> Result<Option<Me>, ApiError> {
        match caller.account {
            Some(account) => self.me(account, caller.channel).await,
            None => Ok(None),
        }
    }
}

impl Handle<ActAs> for Site {
    async fn handle(&self, caller: Caller, acting: Acting) -> Result<Me, ApiError> {
        let account = caller.signed_in()?;
        self.act_as(caller, acting.channel).await?;
        self.me(account, Some(acting.channel))
            .await?
            .ok_or(ApiError::NotFound)
    }
}

impl Handle<SetPreferences> for Site {
    async fn handle(&self, caller: Caller, wanted: Preferences) -> Result<Preferences, ApiError> {
        let account = caller.signed_in()?;
        let language = match wanted.language.as_deref() {
            None => None,
            Some(code) => {
                let lang = youcraft_i18n::i18n()
                    .from_code(code)
                    .ok_or_else(|| ApiError::invalid("language", Rule::NotOffered))?;
                Some(youcraft_i18n::code(lang).to_owned())
            }
        };
        let preferences = Preferences {
            language,
            theme: wanted.theme,
        };
        let stored = preferences.clone();
        self.db(move |store| store.set_preferences(account, &stored))
            .await?;
        Ok(preferences)
    }
}

impl Handle<SignOut> for Site {
    async fn handle(&self, caller: Caller, _: Nothing) -> Result<Nothing, ApiError> {
        if let Credential::Session(hash) = caller.credential {
            self.db(move |store| store.close_session(&hash)).await?;
        }
        Ok(Nothing {})
    }
}
