//! How the site runs each command, one module per area of `youcraft_api::commands`, and what
//! they accept from callers ([`input`]).

mod account;
mod admin;
pub mod channel;
mod comment;
mod feed;
mod history;
pub mod input;
mod invite;
mod member;
mod playlist;
mod profile;
mod search;
mod site;
pub mod studio;
mod token;
pub mod upload;
mod video;
