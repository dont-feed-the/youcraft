//! The bar across the top of every page: the guide button, YouCraft's mark, search, the account.

use leptos::prelude::*;
use youcraft_api::routes;

use crate::app::GuideOpen;
use crate::components::account_menu::AccountArea;
use crate::icons::{Glyph, Icon, Mark};
use crate::lang::tr;

/// The masthead.
#[component]
pub fn Masthead() -> impl IntoView {
    let guide = use_context::<GuideOpen>();
    let toggle = move |_| {
        if let Some(GuideOpen(open)) = guide {
            open.update(|open| *open = !*open);
        }
    };
    view! {
        <header class="masthead">
            <div class="masthead-start">
                <button class="icon-button guide-button" type="button" aria-label=tr("Guide") on:click=toggle>
                    <Glyph icon=Icon::Menu/>
                </button>
                <a class="logo" href=routes::HOME aria-label=tr("YouCraft home")>
                    <Mark/>
                    <span class="wordmark">"YouCraft"</span>
                </a>
            </div>
            <form class="search" action="/results" method="get" role="search">
                <input class="search-input" type="search" name="search_query" placeholder=tr("Search")
                    aria-label=tr("Search") autocomplete="off"/>
                <button class="search-button" type="submit" aria-label=tr("Search")>
                    <Glyph icon=Icon::Search/>
                </button>
            </form>
            <div class="masthead-end">
                <AccountArea/>
            </div>
        </header>
    }
}
