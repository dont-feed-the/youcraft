//! What a page shows when there is nothing there, or when the server could not answer: what
//! went wrong, the way on, and the HTTP status that says it to the browser.

use leptos::prelude::*;
use leptos_router::hooks::use_location;
use youcraft_api::{ApiError, routes};

use crate::lang::tr;
use crate::notice::describe;

/// The page for an address that leads nowhere.
#[component]
pub fn NotFound() -> impl IntoView {
    view! {
        <main class="browse">
            <Trouble error=ApiError::NotFound/>
        </main>
    }
}

/// What a page shows instead of its content when the command it shows failed; the server
/// answers the page with the error's status.
#[component]
pub fn Trouble(error: ApiError) -> impl IntoView {
    answer_with_status_of(&error);
    let location = use_location();
    let here = move || format!("{}{}", location.pathname.get(), location.search.get());
    let way_out = match error {
        ApiError::SignInRequired => view! {
            <a class="pill" rel="external" href=move || routes::sign_in(&here())>{tr("Sign in")}</a>
        }
        .into_any(),
        _ => view! { <a class="pill" href=routes::HOME>{tr("Go to home")}</a> }.into_any(),
    };
    view! {
        <div class="trouble">
            <p>{describe(&error)}</p>
            {way_out}
        </div>
    }
}

/// Have the server answer the page being rendered with the HTTP status of `error`.
pub fn answer_with_status_of(error: &ApiError) {
    #[cfg(feature = "ssr")]
    if let (Some(response), Ok(status)) = (
        use_context::<leptos_axum::ResponseOptions>(),
        http::StatusCode::from_u16(error.status()),
    ) {
        response.set_status(status);
    }
    #[cfg(not(feature = "ssr"))]
    let _ = error;
}
