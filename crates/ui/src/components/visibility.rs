//! Who may see a video or a list: the name of each visibility, and the menu that chooses one.

use leptos::prelude::*;
use youcraft_api::records::Visibility;

use crate::lang::tr;

/// The name of `visibility`.
pub fn visibility_name(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Public => tr("Public"),
        Visibility::Unlisted => tr("Unlisted"),
        Visibility::Private => tr("Private"),
    }
}

/// The menu of visibilities, showing `chosen` and setting it; each option follows the signal,
/// so a menu shown again shows what will be sent.
#[component]
pub fn VisibilityChoice(chosen: RwSignal<Visibility>) -> impl IntoView {
    let choose = move |event: leptos::ev::Event| {
        if let Some(visibility) = Visibility::from_name(&event_target_value(&event)) {
            chosen.set(visibility);
        }
    };
    view! {
        <select class="field" aria-label=tr("Visibility") on:change=choose>
            {Visibility::ALL.map(|visibility| view! {
                <option value=visibility.as_str() selected=chosen.get_untracked() == visibility
                    prop:selected=move || chosen.get() == visibility>
                    {visibility_name(visibility)}
                </option>
            }).collect_view()}
        </select>
    }
}
