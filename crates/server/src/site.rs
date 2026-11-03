//! The running site: its database and data directory, and who is calling.

use std::sync::Arc;

use youcraft_api::{AccountId, ApiError, Role};
use youcraft_store::{Store, StoreError};

use crate::config::Config;

/// Everything a command can reach.
pub struct Site {
    store: Arc<Store>,
}

/// Who runs a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caller {
    /// Their account; `None` when signed out.
    pub account: Option<AccountId>,
    /// What they are to the site.
    pub role: Role,
}

impl Caller {
    /// Someone signed out.
    pub const ANONYMOUS: Caller = Caller {
        account: None,
        role: Role::Anyone,
    };
}

impl Site {
    /// Open the site's data directory and database.
    pub fn open(config: &Config) -> Result<Site, String> {
        std::fs::create_dir_all(&config.data)
            .map_err(|e| format!("{}: {e}", config.data.display()))?;
        let path = config.data.join("youcraft.sqlite");
        let store = Store::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Site {
            store: Arc::new(store),
        })
    }

    /// The database, for work outside a request.
    pub fn store(&self) -> &Store {
        &self.store
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
