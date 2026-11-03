//! The bar across the top of every page: the guide button, YouCraft's mark, search, the account.
//!
//! On phones the search box folds into a button, as on m.youtube.com: pressing it turns the
//! whole bar into the search box, with a way back.

use leptos::html::Input;
use leptos::prelude::*;
use youcraft_api::routes;

use crate::app::GuideOpen;
use crate::components::account_menu::AccountArea;
use crate::icons::{Glyph, Icon, Mark};
use crate::lang::tr;
use crate::pages::results::searched;

/// The masthead.
#[component]
pub fn Masthead() -> impl IntoView {
    let guide = use_context::<GuideOpen>();
    let toggle = move |_| {
        if let Some(GuideOpen(open)) = guide {
            open.update(|open| *open = !*open);
        }
    };
    let searching = RwSignal::new(false);
    let input = NodeRef::<Input>::new();
    let open_search = move |_| {
        searching.set(true);
        request_animation_frame(move || {
            if let Some(input) = input.get_untracked() {
                let _ = input.focus();
            }
        });
    };
    let query = searched();
    view! {
        <header class="masthead" class:searching=move || searching.get()>
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
                <button class="icon-button search-back" type="button" aria-label=tr("Back")
                    on:click=move |_| searching.set(false)>
                    <Glyph icon=Icon::Back/>
                </button>
                <input class="search-input" type="search" name="search_query" placeholder=tr("Search")
                    aria-label=tr("Search") autocomplete="off" maxlength="200" node_ref=input value=move || query.get()
                    prop:value=move || query.get()/>
                <button class="search-button" type="submit" aria-label=tr("Search")>
                    <Glyph icon=Icon::Search/>
                </button>
            </form>
            <div class="masthead-end">
                <button class="icon-button search-open" type="button" aria-label=tr("Search") on:click=open_search>
                    <Glyph icon=Icon::Search/>
                </button>
                <AccountArea/>
            </div>
        </header>
    }
}
