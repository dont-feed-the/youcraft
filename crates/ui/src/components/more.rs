//! Lists read page by page: the first page as the server rendered it, then each next page the
//! reader asks for with the Show more button, as a command's `next` leads.

use leptos::prelude::*;
use youcraft_api::Command;

use crate::act::act;
use crate::lang::tr;

/// The items shown so far, and where the next page starts.
pub struct Pages<T: Send + Sync + 'static> {
    /// The items, in order.
    pub items: RwSignal<Vec<T>>,
    /// The `next` of the last page read; `None` at the end.
    pub next: RwSignal<Option<String>>,
}

impl<T: Send + Sync + 'static> Clone for Pages<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Send + Sync + 'static> Copy for Pages<T> {}

impl<T: Send + Sync + 'static> Pages<T> {
    /// The first page.
    pub fn new(items: Vec<T>, next: Option<String>) -> Pages<T> {
        Pages {
            items: RwSignal::new(items),
            next: RwSignal::new(next),
        }
    }

    /// Read the next page with `C`, whose input `ask` makes from the cursor and whose output
    /// `split` turns into items and the cursor after them.
    pub fn read_next<C: Command>(
        self,
        ask: impl FnOnce(Option<String>) -> C::Input,
        split: impl FnOnce(C::Output) -> (Vec<T>, Option<String>) + 'static,
    ) {
        let Some(after) = self.next.get_untracked() else {
            return;
        };
        act::<C>(ask(Some(after)), move |answer| {
            if let Ok(output) = answer {
                let (items, next) = split(output);
                self.items.update(|all| all.extend(items));
                self.next.set(next);
            }
        });
    }
}

/// The Show more button, while `next` leads somewhere.
#[component]
pub fn ShowMore(next: RwSignal<Option<String>>, more: Callback<()>) -> impl IntoView {
    view! {
        <Show when=move || next.get().is_some()>
            <button class="pill quiet more" type="button" on:click=move |_| more.run(())>{tr("Show more")}</button>
        </Show>
    }
}
