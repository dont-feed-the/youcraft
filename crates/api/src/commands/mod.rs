//! Every command of the site, by area, and the one list of them all.

pub mod account;
pub mod admin;
pub mod channel;
pub mod comment;
pub mod feed;
pub mod history;
pub mod invite;
pub mod member;
pub mod playlist;
pub mod profile;
pub mod search;
pub mod site;
pub mod studio;
pub mod token;
pub mod upload;
pub mod video;

/// Call `$callback!` with the path of every command type, once each. Add a new command here, in
/// its area's place; the server then refuses to compile until it handles it.
#[macro_export]
macro_rules! for_each_command {
    ($callback:ident) => {
        $callback! {
            $crate::commands::site::ListCommands,
            $crate::commands::site::GetPolicy,
            $crate::commands::feed::Home,
            $crate::commands::video::Get,
            $crate::commands::video::Next,
            $crate::commands::video::SetRating,
            $crate::commands::video::View,
            $crate::commands::feed::Subscriptions,
            $crate::commands::channel::Get,
            $crate::commands::channel::Create,
            $crate::commands::channel::Delete,
            $crate::commands::channel::Videos,
            $crate::commands::channel::Playlists,
            $crate::commands::channel::Subscribe,
            $crate::commands::channel::Subscriptions,
            $crate::commands::channel::Block,
            $crate::commands::profile::Update,
            $crate::commands::profile::SetPhoto,
            $crate::commands::profile::RemovePhoto,
            $crate::commands::comment::List,
            $crate::commands::comment::Replies,
            $crate::commands::comment::Create,
            $crate::commands::comment::Update,
            $crate::commands::comment::Delete,
            $crate::commands::comment::Like,
            $crate::commands::history::List,
            $crate::commands::history::Remove,
            $crate::commands::history::Clear,
            $crate::commands::history::Pause,
            $crate::commands::playlist::Create,
            $crate::commands::playlist::Update,
            $crate::commands::playlist::Delete,
            $crate::commands::playlist::Add,
            $crate::commands::playlist::Remove,
            $crate::commands::playlist::Move,
            $crate::commands::playlist::Lists,
            $crate::commands::playlist::Get,
            $crate::commands::search::Search,
            $crate::commands::account::Get,
            $crate::commands::account::ActAs,
            $crate::commands::account::SetPreferences,
            $crate::commands::account::SignOut,
            $crate::commands::token::Create,
            $crate::commands::token::List,
            $crate::commands::token::Revoke,
            $crate::commands::member::List,
            $crate::commands::member::Invite,
            $crate::commands::member::Uninvite,
            $crate::commands::member::SetRole,
            $crate::commands::member::Remove,
            $crate::commands::member::Offers,
            $crate::commands::member::Accept,
            $crate::commands::member::Decline,
            $crate::commands::invite::Create,
            $crate::commands::invite::List,
            $crate::commands::invite::Revoke,
            $crate::commands::admin::SetRole,
            $crate::commands::admin::Block,
            $crate::commands::admin::Accounts,
            $crate::commands::admin::ResetName,
            $crate::commands::admin::RemovePhoto,
            $crate::commands::video::Update,
            $crate::commands::video::SetThumbnail,
            $crate::commands::video::RemoveThumbnail,
            $crate::commands::video::Delete,
            $crate::commands::upload::Create,
            $crate::commands::upload::Status,
            $crate::commands::upload::Finish,
            $crate::commands::studio::Videos,
            $crate::commands::studio::Video,
        }
    };
}
