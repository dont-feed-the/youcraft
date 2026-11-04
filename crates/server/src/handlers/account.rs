//! Commands about the signed-in person's own account.

use youcraft_api::commands::account::{Get, Me, Preferences, SetPreferences, SignOut};
use youcraft_api::commands::site::Nothing;
use youcraft_api::{ApiError, Rule};

use crate::dispatch::Handle;
use crate::site::{Caller, Credential, Site};

impl Handle<Get> for Site {
    async fn handle(&self, caller: Caller, _: Nothing) -> Result<Option<Me>, ApiError> {
        match caller.account {
            Some(account) => {
                let me = self.db(move |store| store.me(account)).await?;
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
            None => Ok(None),
        }
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
