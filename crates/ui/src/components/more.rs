//! Lists read page by page: the first page as the server rendered it (or as the browser read it),
//! then each next page the reader asks for with the Show more button, as a command's `next`
//! leads.
//!
//! One page is read at a time, and starting the list again (another order, say) makes every
//! answer still on its way for the old list arrive to nothing. Items are told apart by their
//! [`Keyed::key`]: a page that repeats an item already shown (the list moved meanwhile) adds only
//! what is new.

use leptos::prelude::*;
use youcraft_api::Command;
use youcraft_api::commands::admin::AccountRow;
use youcraft_api::commands::comment::Comment;
use youcraft_api::commands::history::Watched;
use youcraft_api::commands::playlist::PlaylistCard;
use youcraft_api::records::{StudioVideo, VideoCard};
use youcraft_api::{ChannelId, CommentId, List, VideoId};

use crate::act::Scope;
use crate::lang::tr;

/// An item of a list, told apart from the others by its key.
pub trait Keyed {
    /// What tells it apart.
    type Key: PartialEq + Clone + Send + Sync + 'static;
    /// Its key.
    fn key(&self) -> Self::Key;
}

impl Keyed for VideoCard {
    type Key = VideoId;
    fn key(&self) -> VideoId {
        self.id
    }
}

impl Keyed for Watched {
    type Key = VideoId;
    fn key(&self) -> VideoId {
        self.card.id
    }
}

impl Keyed for StudioVideo {
    type Key = VideoId;
    fn key(&self) -> VideoId {
        self.card.id
    }
}

impl Keyed for PlaylistCard {
    type Key = List;
    fn key(&self) -> List {
        self.list
    }
}

impl Keyed for Comment {
    type Key = CommentId;
    fn key(&self) -> CommentId {
        self.id
    }
}

impl Keyed for AccountRow {
    type Key = ChannelId;
    fn key(&self) -> ChannelId {
        self.channel.id
    }
}

/// `items` followed by those of `more` it does not hold yet, in their order.
pub fn joined<T: Keyed>(items: &mut Vec<T>, more: Vec<T>) {
    for item in more {
        let key = item.key();
        if !items.iter().any(|held| held.key() == key) {
            items.push(item);
        }
    }
}

/// `item` in place of the item of `items` with its key, if there is one.
pub fn replaced<T: Keyed>(items: &mut [T], item: T) {
    let key = item.key();
    if let Some(held) = items.iter_mut().find(|held| held.key() == key) {
        *held = item;
    }
}

/// The items shown so far, where the next page starts, and whether a page is on its way.
pub struct Pages<T: Send + Sync + 'static> {
    /// The items, in order.
    pub items: RwSignal<Vec<T>>,
    /// The `next` of the last page read; `None` at the end.
    pub next: RwSignal<Option<String>>,
    /// Whether a page is on its way.
    pub reading: RwSignal<bool>,
    /// How many times the list started again; an answer for an older start is dropped.
    start: StoredValue<u64>,
    /// The part of the page that shows the list.
    scope: StoredValue<Scope>,
}

impl<T: Send + Sync + 'static> Clone for Pages<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Send + Sync + 'static> Copy for Pages<T> {}

impl<T: Keyed + Send + Sync + 'static> Pages<T> {
    /// The first page.
    pub fn new(items: Vec<T>, next: Option<String>) -> Pages<T> {
        Pages {
            items: RwSignal::new(items),
            next: RwSignal::new(next),
            reading: RwSignal::new(false),
            start: StoredValue::new(0),
            scope: StoredValue::new(Scope::current()),
        }
    }

    /// Start the list again with the first page `C` answers to `first`, which `split` turns into
    /// items and the cursor after them; until it comes, the list shows what it showed.
    pub fn read_first<C: Command>(
        self,
        first: C::Input,
        split: impl FnOnce(C::Output) -> (Vec<T>, Option<String>) + 'static,
    ) {
        let start = self.start.get_value().saturating_add(1);
        self.start.set_value(start);
        self.reading.set(true);
        self.scope.get_value().act::<C>(first, move |answer| {
            if self.start.get_value() != start {
                return;
            }
            self.reading.set(false);
            if let Ok(output) = answer {
                let (items, next) = split(output);
                self.items.set(items);
                self.next.set(next);
            }
        });
    }

    /// Read the next page with `C`, whose input `ask` makes from the cursor and whose output
    /// `split` turns into items and the cursor after them; nothing while a page is on its way.
    pub fn read_next<C: Command>(
        self,
        ask: impl FnOnce(Option<String>) -> C::Input,
        split: impl FnOnce(C::Output) -> (Vec<T>, Option<String>) + 'static,
    ) {
        if self.reading.get_untracked() {
            return;
        }
        let Some(after) = self.next.get_untracked() else {
            return;
        };
        let start = self.start.get_value();
        self.reading.set(true);
        self.scope
            .get_value()
            .act::<C>(ask(Some(after)), move |answer| {
                if self.start.get_value() != start {
                    return;
                }
                self.reading.set(false);
                if let Ok(output) = answer {
                    let (items, next) = split(output);
                    self.items.update(|all| joined(all, items));
                    self.next.set(next);
                }
            });
    }

    /// Read an item again with `C`, quietly, and show what `item` makes of the answer in place of
    /// the item with its key.
    pub fn read_again<C: Command>(
        self,
        input: C::Input,
        item: impl FnOnce(C::Output) -> T + 'static,
    ) {
        self.scope.get_value().ask::<C>(input, move |answer| {
            if let Ok(output) = answer {
                self.replace(item(output));
            }
        });
    }

    /// Show `item` first.
    pub fn put_first(self, item: T) {
        self.items.update(|items| {
            let key = item.key();
            items.retain(|held| held.key() != key);
            items.insert(0, item);
        });
    }

    /// Show `item` last.
    pub fn put_last(self, item: T) {
        self.items.update(|items| {
            let key = item.key();
            items.retain(|held| held.key() != key);
            items.push(item);
        });
    }

    /// Show `item` in place of the item with its key.
    pub fn replace(self, item: T) {
        self.items.update(|items| replaced(items, item));
    }

    /// Take the item with `key` out of the list.
    pub fn remove(self, key: &T::Key) {
        self.items
            .update(|items| items.retain(|held| held.key() != *key));
    }
}

/// The Show more button, while `next` leads somewhere; it waits while a page is on its way.
#[component]
pub fn ShowMore<T: Keyed + Send + Sync + 'static>(
    pages: Pages<T>,
    more: Callback<()>,
    /// Its label, Show more by default.
    #[prop(optional)]
    label: Option<&'static str>,
    /// Its class besides `more`, a small pill by default.
    #[prop(optional)]
    small: bool,
) -> impl IntoView {
    let label = label.unwrap_or_else(|| tr("Show more"));
    view! {
        <Show when=move || pages.next.get().is_some()>
            <button class="pill quiet more" class:small=small type="button"
                disabled=move || pages.reading.get() on:click=move |_| more.run(())>
                {label}
            </button>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct Item(u32, &'static str);

    impl Keyed for Item {
        type Key = u32;
        fn key(&self) -> u32 {
            self.0
        }
    }

    #[test]
    fn a_next_page_adds_only_what_is_new() {
        let mut items = vec![Item(3, "c"), Item(2, "b")];
        joined(&mut items, vec![Item(2, "b again"), Item(1, "a")]);
        assert_eq!(items, vec![Item(3, "c"), Item(2, "b"), Item(1, "a")]);
    }

    #[test]
    fn a_fresh_item_replaces_its_own_in_place() {
        let mut items = vec![Item(3, "c"), Item(2, "b")];
        replaced(&mut items, Item(2, "b, newer"));
        replaced(&mut items, Item(9, "not listed"));
        assert_eq!(items, vec![Item(3, "c"), Item(2, "b, newer")]);
    }

    #[test]
    fn items_are_put_first_or_last_once_and_taken_out_by_key() {
        let owner = Owner::new();
        owner.with(|| {
            let pages = Pages::new(vec![Item(1, "a")], Some("1".to_owned()));
            pages.put_first(Item(0, "z"));
            pages.put_last(Item(0, "z"));
            assert_eq!(
                pages.items.get_untracked(),
                vec![Item(1, "a"), Item(0, "z")]
            );
            pages.remove(&1);
            assert_eq!(pages.items.get_untracked(), vec![Item(0, "z")]);
        });
    }
}
