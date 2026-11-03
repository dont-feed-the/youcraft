//! Commands about the site itself.

use youcraft_api::commands::site::{GetPolicy, ListCommands, Nothing, Policy};
use youcraft_api::{ApiError, Descriptor, command};

use crate::dispatch::Handle;
use crate::site::{Caller, Site};

impl Handle<ListCommands> for Site {
    async fn handle(&self, _: Caller, _: Nothing) -> Result<Vec<Descriptor>, ApiError> {
        Ok(command::descriptors())
    }
}

impl Handle<GetPolicy> for Site {
    async fn handle(&self, _: Caller, _: Nothing) -> Result<Policy, ApiError> {
        Ok(Policy {
            signed_in_role: self.config().signed_in_role,
        })
    }
}
