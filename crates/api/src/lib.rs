//! The contract between YouCraft's browser code and its server.
//!
//! Identifiers as they appear in addresses, the records pages show, the URL scheme, the layout
//! of the skim sheet that the transcoder writes and pages read ([`skim`]), and the commands:
//! every action anyone can take on the site, each with an id, typed input and output, and the
//! role it requires. The pages, the HTTP API, the command line and the MCP server all
//! dispatch these same commands. Nothing here does I/O, so it builds for the browser too.

pub mod command;
pub mod commands;
pub mod cookies;
pub mod error;
pub mod ids;
pub mod records;
pub mod role;
pub mod routes;
pub mod skim;
pub mod time;

pub use command::{Answer, Command, Descriptor, Dispatch};
pub use error::{ApiError, Rule};
pub use ids::{AccountId, ChannelId, CommentId, List, PlaylistId, TokenId, VideoId};
pub use role::{ChannelRole, Grant, Role};
