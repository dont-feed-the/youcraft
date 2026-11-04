//! The running site: its database, configuration and data directory, and who is calling.

use std::sync::Arc;

use youcraft_api::{AccountId, ApiError, Grant, Role};
use youcraft_store::accounts::Standing;
use youcraft_store::credentials::SecretHash;
use youcraft_store::{Reader, Store, StoreError};

use crate::config::Config;

/// Everything a command can reach.
pub struct Site {
    store: Arc<Store>,
    config: Config,
    /// Wakes the job worker when an upload is finished.
    pub work: tokio::sync::Notify,
    /// Held while bytes of an upload are written, so two pieces never interleave.
    pub writing: tokio::sync::Mutex<()>,
    salt: [u8; 32],
}

/// Who runs a command. Handlers decide with it what the caller may do: [`Caller::role`] for
/// what the site lets them do at all, [`Caller::signed_in`] for whose things they act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caller {
    /// Their account; `None` when signed out, and for the operator.
    pub account: Option<AccountId>,
    /// What they may do: their account's role, raised to the site's floor, or
    /// [`Role::SignedIn`] while it is blocked.
    pub role: Role,
    /// How they proved who they are.
    pub credential: Credential,
    /// Who they are to the view counter.
    pub viewer: ViewerKey,
}

/// Who a caller is to the view counter, which counts one view a day per viewer: a hash of their
/// account, or of the address and browser a visitor comes from, salted with a secret the server
/// draws when it starts, so that it names nobody and links nothing across restarts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewerKey(pub [u8; 32]);

impl ViewerKey {
    /// No one in particular: the operator, and requests the server did not resolve.
    pub const NOBODY: ViewerKey = ViewerKey([0; 32]);
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
        viewer: ViewerKey::NOBODY,
    };

    /// Whoever runs `youcraft` on the server's machine: an administrator with no account.
    pub const OPERATOR: Caller = Caller {
        account: None,
        role: Role::Admin,
        credential: Credential::Operator,
        viewer: ViewerKey::NOBODY,
    };

    /// The caller whose credential shows `standing`, known to the view counter as `viewer`, on
    /// a site that grants every account at least `floor`.
    pub fn of(
        standing: &Standing,
        floor: Grant,
        credential: Credential,
        viewer: ViewerKey,
    ) -> Caller {
        Caller {
            account: Some(standing.account),
            role: if standing.blocked {
                Role::SignedIn
            } else {
                Role::from(standing.role.max(floor))
            },
            credential,
            viewer,
        }
    }

    /// The caller's account, or [`ApiError::SignInRequired`] if they have none.
    pub fn signed_in(&self) -> Result<AccountId, ApiError> {
        self.account.ok_or(ApiError::SignInRequired)
    }

    /// The caller as the store sees who reads: their account, and whether they run the site.
    pub fn reader(&self) -> Reader {
        Reader {
            account: self.account,
            admin: self.role.allows(Role::Admin),
        }
    }

    /// Allow the caller to act toward others, as a command that also lets them take back what
    /// they did (a like, a subscription) checks before anything else: a blocked account may take
    /// back, not act.
    pub fn may_act(&self) -> Result<(), ApiError> {
        if self.role.allows(Role::Viewer) {
            Ok(())
        } else {
            Err(ApiError::Forbidden)
        }
    }
}

impl Site {
    /// Open the site's data directory and database.
    pub fn open(config: &Config) -> Result<Site, String> {
        for dir in [
            config.data.clone(),
            config.uploads(),
            config.originals(),
            config.work(),
            config.media(),
        ] {
            std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let path = config.data.join("youcraft.sqlite");
        let store = Store::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Site::with(store, config.clone()))
    }

    /// The site over `store`.
    pub fn with(store: Store, config: Config) -> Site {
        Site {
            store: Arc::new(store),
            config,
            work: tokio::sync::Notify::new(),
            writing: tokio::sync::Mutex::new(()),
            salt: rand::random(),
        }
    }

    /// The database, for work outside a request.
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// The secret that salts [`ViewerKey`]s, drawn when the site opened.
    pub fn salt(&self) -> &[u8; 32] {
        &self.salt
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
