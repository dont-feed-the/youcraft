//! What a page shows when there is nothing there, or when the server could not answer, and what
//! a person reads about a rule their input broke.

use leptos::prelude::*;
use leptos_router::hooks::use_location;
use youcraft_api::{ApiError, Rule, routes};

use crate::lang::{count, tr};

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
    let text = match &error {
        ApiError::NotFound | ApiError::UnknownCommand => {
            tr("This page isn't available.").to_owned()
        }
        ApiError::SignInRequired => tr("Sign in to see this page.").to_owned(),
        ApiError::Forbidden => tr("You can't see this page.").to_owned(),
        ApiError::Invalid { rule, .. } => describe_rule(rule),
        _ => tr("Something went wrong. Try again later.").to_owned(),
    };
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
            <p>{text}</p>
            {way_out}
        </div>
    }
}

/// What a person reads about `rule`, which something they wrote or chose breaks.
pub fn describe_rule(rule: &Rule) -> String {
    match rule {
        Rule::Required => tr("This can't be empty.").to_owned(),
        Rule::TooLong { limit } => {
            count(*limit, "{n} character at most.", "{n} characters at most.")
        }
        Rule::OneLine => tr("This must fit on one line.").to_owned(),
        Rule::Characters => tr("This holds characters that aren't allowed.").to_owned(),
        Rule::Malformed => tr("This isn't in the expected form.").to_owned(),
        Rule::NotOffered => tr("This choice isn't offered here.").to_owned(),
        // Pages send what the schema asks; a mismatch is the site's own failure.
        Rule::Schema { .. } => tr("Something went wrong. Try again later.").to_owned(),
    }
}
