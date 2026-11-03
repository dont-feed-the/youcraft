//! What a page shows when there is nothing there, or when the server could not answer.

use leptos::prelude::*;
use youcraft_api::{ApiError, routes};

use crate::lang::tr;

/// The page for an address that leads nowhere; the server answers it with 404.
#[component]
pub fn NotFound() -> impl IntoView {
    #[cfg(feature = "ssr")]
    if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
        response.set_status(http::StatusCode::NOT_FOUND);
    }
    view! {
        <main class="trouble">
            <p>{tr("This page isn't available.")}</p>
            <a class="pill" href=routes::HOME>{tr("Go to home")}</a>
        </main>
    }
}

/// What a page shows instead of its content when a command failed.
#[component]
pub fn Trouble(error: ApiError) -> impl IntoView {
    let text = match error {
        ApiError::NotFound | ApiError::UnknownCommand => tr("This page isn't available."),
        ApiError::SignInRequired => tr("Sign in to see this page."),
        ApiError::Forbidden => tr("You can't see this page."),
        _ => tr("Something went wrong. Try again later."),
    };
    view! {
        <div class="trouble">
            <p>{text}</p>
            <a class="pill" href=routes::HOME>{tr("Go to home")}</a>
        </div>
    }
}
