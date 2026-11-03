//! The cookies the browser itself writes, which the server reads: an anonymous visitor's
//! preferences. (The server's own cookies, which scripts cannot read, are the server's business.)

/// The code of the language the visitor chose.
pub const LANGUAGE: &str = "language";
/// The theme the visitor chose, as [`crate::commands::account::Theme::as_str`] writes it.
pub const THEME: &str = "theme";
/// How long the browser keeps them, in seconds: 400 days, the most browsers allow.
pub const MAX_AGE: u32 = 400 * 86_400;
