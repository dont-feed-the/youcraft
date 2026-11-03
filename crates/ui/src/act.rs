//! Acting from a click: run a command in the background, and take a signed-out person to sign
//! in first, then back to the page they were on.

use youcraft_api::{ApiError, Command, routes};

use crate::browser;
use crate::call::call;

/// Run `C` on `input`, then `done` with its answer; a caller who has to sign in first is sent
/// to sign in, and `done` hears the error too, to undo what it showed in advance.
pub fn act<C: Command>(input: C::Input, done: impl FnOnce(Result<C::Output, ApiError>) + 'static) {
    leptos::task::spawn_local(async move {
        let answer = call::<C>(input).await;
        if answer.as_ref().err() == Some(&ApiError::SignInRequired) {
            browser::go(&routes::sign_in(&browser::here()));
        }
        done(answer);
    });
}
