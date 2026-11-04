//! Running commands from a page in the browser: [`ask`] quietly, as a page following a video's
//! progress does; [`act`] for what the person asked for, taking a signed-out person to sign in
//! first, then back to the page they were on, and saying every other failure in the page's live
//! region ([`crate::notice`]); and [`Optimistic`], to show an action's outcome in advance and
//! settle on what the site answers.
//!
//! An answer reaches the component that asked only while it is shown: a person may leave the page
//! before the site answers, and the component's signals and callbacks are gone by then.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use leptos::prelude::*;
use leptos::reactive::owner::WeakOwner;
use youcraft_api::{ApiError, Command, routes};

use crate::browser;
use crate::call::call;
use crate::notice::{Notices, describe};

/// Whether the component being built, or whose event is being handled, is still shown: it is
/// until its owner cleans up, which disposes its signals and callbacks.
struct StillShown(Arc<AtomicBool>);

impl StillShown {
    /// For the current component; outside any, for good.
    fn here() -> StillShown {
        let shown = Arc::new(AtomicBool::new(true));
        let hidden = Arc::clone(&shown);
        on_cleanup(move || hidden.store(false, Ordering::Relaxed));
        StillShown(shown)
    }

    /// Whether it is.
    fn holds(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Run `C` on `input`, then `done` with its answer if the component that asked is still shown.
pub fn ask<C: Command>(input: C::Input, done: impl FnOnce(Result<C::Output, ApiError>) + 'static) {
    let shown = StillShown::here();
    leptos::task::spawn_local(async move {
        let answer = call::<C>(input).await;
        if shown.holds() {
            done(answer);
        }
    });
}

/// [`ask`] what the person asked for: a caller who has to sign in first is sent to sign in, any
/// other failure is said, and `done` hears the error too, to undo what it showed in advance.
pub fn act<C: Command>(input: C::Input, done: impl FnOnce(Result<C::Output, ApiError>) + 'static) {
    let notices = use_context::<Notices>();
    ask::<C>(input, move |answer| {
        match &answer {
            Err(ApiError::SignInRequired) => browser::go(&routes::sign_in(&browser::here())),
            Err(error) => {
                if let Some(notices) = notices {
                    notices.say(describe(error));
                }
            }
            Ok(_) => {}
        }
        done(answer);
    });
}

/// The part of the page that answers are for: the component that holds what they change, so
/// that an answer reaches it for as long as it is shown, whichever of its children asked.
#[derive(Clone)]
pub struct Scope(Option<WeakOwner>);

impl Scope {
    /// The component being built, or whose event is being handled.
    pub fn current() -> Scope {
        Scope(Owner::current().map(|owner| owner.downgrade()))
    }

    /// Run `then` inside this part of the page, as a continuation after an `await` must; nothing
    /// once it is gone.
    pub fn within(&self, then: impl FnOnce()) {
        match &self.0 {
            Some(owner) => {
                if let Some(owner) = owner.upgrade() {
                    owner.with(then);
                }
            }
            None => then(),
        }
    }

    /// [`ask`] for this part of the page.
    pub fn ask<C: Command>(
        &self,
        input: C::Input,
        done: impl FnOnce(Result<C::Output, ApiError>) + 'static,
    ) {
        self.within(|| ask::<C>(input, done));
    }

    /// [`act`] for this part of the page.
    pub fn act<C: Command>(
        &self,
        input: C::Input,
        done: impl FnOnce(Result<C::Output, ApiError>) + 'static,
    ) {
        self.within(|| act::<C>(input, done));
    }
}

/// A state the page shows before the site confirms it, as a like or a subscription: each change
/// shows what is wanted at once, and the answer of the latest change settles it. An answer that
/// a later change overtook only moves what is confirmed; a failure of the latest change shows
/// the last confirmed state again.
pub struct Optimistic<S: Send + Sync + 'static> {
    /// What the page shows.
    pub shown: RwSignal<S>,
    /// The state the site last confirmed, and the number of the change that confirmed it.
    confirmed: StoredValue<(u64, S)>,
    /// The number of the latest change.
    latest: StoredValue<u64>,
    /// The part of the page that shows it.
    scope: StoredValue<Scope>,
}

impl<S: Send + Sync + 'static> Clone for Optimistic<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S: Send + Sync + 'static> Copy for Optimistic<S> {}

impl<S: Clone + Send + Sync + 'static> Optimistic<S> {
    /// A state the site has confirmed.
    pub fn new(state: S) -> Optimistic<S> {
        Optimistic {
            shown: RwSignal::new(state.clone()),
            confirmed: StoredValue::new((0, state)),
            latest: StoredValue::new(0),
            scope: StoredValue::new(Scope::current()),
        }
    }

    /// Show `wanted`, and ask for it with `C` on `input`; `settle` reads the state from the answer.
    pub fn change<C: Command>(
        self,
        wanted: S,
        input: C::Input,
        settle: impl FnOnce(C::Output) -> S + 'static,
    ) {
        let number = self.latest.get_value().saturating_add(1);
        self.latest.set_value(number);
        self.shown.set(wanted);
        let scope = self.scope.get_value();
        scope.act::<C>(input, move |answer| {
            let latest = self.latest.get_value();
            let shown = self.confirmed.try_update_value(|confirmed| {
                settled(number, latest, confirmed, answer.map(settle).ok())
            });
            if let Some(Some(state)) = shown {
                self.shown.set(state);
            }
        });
    }
}

/// What a page shows once the answer to change `number` comes, `None` to keep what it shows,
/// while `latest` is the number of the latest change and `confirmed` the last confirmed state
/// with the number of its change, which a newer answer moves. `answer` is the state the site
/// answered, `None` if the change failed.
fn settled<S: Clone>(
    number: u64,
    latest: u64,
    confirmed: &mut (u64, S),
    answer: Option<S>,
) -> Option<S> {
    match answer {
        Some(state) => {
            if confirmed.0 < number {
                *confirmed = (number, state.clone());
            }
            (number == latest).then_some(state)
        }
        None => (number == latest).then(|| confirmed.1.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_reaches_a_component_only_while_it_is_shown() {
        let page = Owner::new();
        let shown = page.with(StillShown::here);
        assert!(shown.holds());
        page.cleanup();
        assert!(
            !shown.holds(),
            "the person left the page before the answer came"
        );
        assert!(
            StillShown::here().holds(),
            "outside any component, for good"
        );
    }

    #[test]
    fn a_part_of_the_page_that_is_gone_runs_nothing() {
        let page = Owner::new();
        let scope = page.with(Scope::current);
        let mut ran = 0;
        scope.within(|| ran += 1);
        drop(page);
        scope.within(|| ran += 1);
        assert_eq!(ran, 1);
    }

    #[test]
    fn the_latest_change_settles_what_is_shown() {
        // Liked (1), then unliked (2): the answers come in order.
        let mut confirmed = (0, false);
        assert_eq!(settled(1, 2, &mut confirmed, Some(true)), None);
        assert_eq!(confirmed, (1, true));
        assert_eq!(settled(2, 2, &mut confirmed, Some(false)), Some(false));
        assert_eq!(confirmed, (2, false));
    }

    #[test]
    fn an_overtaken_answer_moves_only_what_is_confirmed() {
        // Change 2's answer comes before change 1's, which says less.
        let mut confirmed = (0, false);
        assert_eq!(settled(2, 2, &mut confirmed, Some(false)), Some(false));
        assert_eq!(settled(1, 2, &mut confirmed, Some(true)), None);
        assert_eq!(
            confirmed,
            (2, false),
            "the older answer says less than the newer"
        );
    }

    #[test]
    fn a_failed_latest_change_shows_the_last_confirmed_state() {
        let mut confirmed = (0, false);
        assert_eq!(settled(1, 2, &mut confirmed, Some(true)), None);
        assert_eq!(settled(2, 2, &mut confirmed, None), Some(true));
        assert_eq!(
            settled(1, 3, &mut confirmed, None),
            None,
            "a later change is on its way"
        );
    }
}
