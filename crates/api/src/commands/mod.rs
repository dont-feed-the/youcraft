//! Every command of the site, by area, and the one list of them all.

pub mod feed;
pub mod site;
pub mod video;

/// Call `$callback!` with the path of every command type, once each. Add a new command here, in
/// its area's place; the server then refuses to compile until it handles it.
#[macro_export]
macro_rules! for_each_command {
    ($callback:ident) => {
        $callback! {
            $crate::commands::site::ListCommands,
            $crate::commands::feed::Home,
            $crate::commands::video::Get,
            $crate::commands::video::Next,
        }
    };
}
