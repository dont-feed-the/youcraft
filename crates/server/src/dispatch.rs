//! Running commands by id: the role check, the rate of a command that writes, the JSON on both
//! sides, and the handler.
//!
//! The dispatch table is built from `youcraft_api::for_each_command!`, so every command the API
//! lists must have a [`Handle`] implementation here, or the server does not compile.

use std::future::Future;
use std::sync::Arc;

use serde_json::Value;
use youcraft_api::{Answer, ApiError, Command, Dispatch};

use crate::site::{Caller, Site};

/// How the site runs the command `C`.
pub trait Handle<C: Command> {
    /// Run `C` for `caller`, whose role already allows it.
    fn handle(
        &self,
        caller: Caller,
        input: C::Input,
    ) -> impl Future<Output = Result<C::Output, ApiError>> + Send;
}

async fn run_one<C: Command>(site: &Site, caller: Caller, input: Value) -> Result<Value, ApiError>
where
    Site: Handle<C>,
{
    if !caller.role.allows(C::REQUIRES) {
        return Err(if caller.account.is_none() {
            ApiError::SignInRequired
        } else {
            ApiError::Forbidden
        });
    }
    if C::WRITES {
        site.spend(&caller)?;
    }
    let input: C::Input = serde_json::from_value(input).map_err(|e| ApiError::schema(&e))?;
    let output = <Site as Handle<C>>::handle(site, caller, input).await?;
    serde_json::to_value(output).map_err(|_| ApiError::Internal)
}

macro_rules! dispatch_table {
    ($($command:path),* $(,)?) => {
        /// Run the command `id` for `caller` on `input`.
        pub async fn run(site: &Site, caller: Caller, id: &str, input: Value) -> Result<Value, ApiError> {
            $(
                if id == <$command as Command>::ID {
                    return run_one::<$command>(site, caller, input).await;
                }
            )*
            Err(ApiError::UnknownCommand)
        }
    };
}

youcraft_api::for_each_command!(dispatch_table);

/// The site, bound to the caller of one request, for the pages the server renders.
pub struct RequestDispatch {
    /// The site.
    pub site: Arc<Site>,
    /// Who made the request.
    pub caller: Caller,
}

impl Dispatch for RequestDispatch {
    fn dispatch(&self, id: &str, input: Value) -> Answer {
        let (site, caller, id) = (Arc::clone(&self.site), self.caller, id.to_owned());
        Box::pin(async move { run(&site, caller, &id, input).await })
    }
}
