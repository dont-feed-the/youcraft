//! The running site: its database, configuration and data directory, and who is calling.

use std::sync::Arc;

use youcraft_api::{AccountId, ApiError, Role};
use youcraft_store::accounts::Standing;
use youcraft_store::credentials::SecretHash;
use youcraft_store::{Store, StoreError};

use crate::config::Config;

/// Everything a command can reach.
pub struct Site {
    store: Arc<Store>,
    config: Config,
}

/// Who runs a command. Handlers decide with it what the caller may do: [`Caller::role`] for
/// what the site lets them do at all, [`Caller::signed_in`] for whose things they act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caller {
    /// Their account; `None` when signed out, and for the operator.
    pub account: Option<AccountId>,
    /// What they may do: their account's role, or [`Role::Anyone`] while it is blocked.
    pub role: Role,
    /// How they proved who they are.
    pub credential: Credential,
}

/// How a caller proved who they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Credential {
    /// They did not: signed out.
    Nothing,
    /// The session cookie of a browser, by its hash.
    Session(SecretHash),
    /// A personal token.
    Token,
    /// They run the server's command line, on its machine.
    Operator,
}

impl Caller {
    /// Someone signed out.
    pub const ANONYMOUS: Caller = Caller {
        account: None,
        role: Role::Anyone,
        credential: Credential::Nothing,
    };

    /// Whoever runs `youcraft` on the server's machine: an administrator with no account.
    pub const OPERATOR: Caller = Caller {
        account: None,
        role: Role::Admin,
        credential: Credential::Operator,
    };

    /// The caller whose credential shows `standing`.
    pub fn of(standing: &Standing, credential: Credential) -> Caller {
        Caller {
            account: Some(standing.account),
            role: if standing.blocked {
                Role::Anyone
            } else {
                standing.role
            },
            credential,
        }
    }

    /// The caller's account, or [`ApiError::SignInRequired`] if they have none.
    pub fn signed_in(&self) -> Result<AccountId, ApiError> {
        self.account.ok_or(ApiError::SignInRequired)
    }
}

impl Site {
    /// Open the site's data directory and database.
    pub fn open(config: &Config) -> Result<Site, String> {
        std::fs::create_dir_all(&config.data)
            .map_err(|e| format!("{}: {e}", config.data.display()))?;
        let path = config.data.join("youcraft.sqlite");
        let store = Store::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Site::with(store, config.clone()))
    }

    /// The site over `store`.
    pub fn with(store: Store, config: Config) -> Site {
        Site {
            store: Arc::new(store),
            config,
        }
    }

    /// The database, for work outside a request.
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// The configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Run `query` on the database off the async threads; a failure is logged and answered as
    /// [`ApiError::Internal`].
    pub async fn db<T, F>(&self, query: F) -> Result<T, ApiError>
    where
        T: Send + 'static,
        F: FnOnce(&Store) -> Result<T, StoreError> + Send + 'static,
    {
        let store = Arc::clone(&self.store);
        match tokio::task::spawn_blocking(move || query(&store)).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => {
                tracing::error!("database: {error}");
                Err(ApiError::Internal)
            }
            Err(error) => {
                tracing::error!("database task: {error}");
                Err(ApiError::Internal)
            }
        }
    }
}

/// Seconds since 1970, now.
pub fn now() -> i64 {
    let elapsed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
    elapsed.map_or(0, |elapsed| {
        i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
    })
}
