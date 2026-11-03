//! Commands about the site itself.

use youcraft_api::commands::site::{ListCommands, Nothing};
use youcraft_api::{ApiError, Descriptor, command};

use crate::dispatch::Handle;
use crate::site::{Caller, Site};

impl Handle<ListCommands> for Site {
    async fn handle(&self, _: Caller, _: Nothing) -> Result<Vec<Descriptor>, ApiError> {
        Ok(command::descriptors())
    }
}
