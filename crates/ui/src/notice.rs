//! What the site tells a person about what they asked for: what each failure means, in their
//! language ([`describe`]), and the one live region of the page that says it ([`NoticeRegion`]),
//! a short note at the bottom that screen readers announce.
//!
//! The region is always in the page, empty between notes, so that each note put in it is read
//! out; a note goes away after a few seconds, unless a newer one replaced it first.

use leptos::prelude::*;
use youcraft_api::{ApiError, Rule};

use crate::lang::{count, tr};

/// How long a note shows.
const SHOWN_FOR: std::time::Duration = std::time::Duration::from_secs(5);

/// What `error` means to the person who met it, on a page or after an action.
pub fn describe(error: &ApiError) -> String {
    match error {
        ApiError::SignInRequired => tr("Sign in first.").to_owned(),
        ApiError::Forbidden => tr("This isn't open to your account.").to_owned(),
        ApiError::ChannelRequired => tr("Create a channel first.").to_owned(),
        ApiError::NotFound | ApiError::UnknownCommand => tr("This isn't available.").to_owned(),
        // the one file a command carries, a channel's picture
        ApiError::Invalid {
            field,
            rule: Rule::Malformed,
        } if field == "image" => {
            tr("This picture can't be read. Choose a JPEG, PNG or WebP file.").to_owned()
        }
        ApiError::Invalid { rule, .. } => describe_rule(rule),
        ApiError::TooLarge { .. } => tr("This is larger than YouCraft accepts.").to_owned(),
        ApiError::Conflict => tr("This conflicts with what is already there.").to_owned(),
        ApiError::TooOften { .. } => {
            tr("Too many attempts. Wait a moment and try again.").to_owned()
        }
        ApiError::Internal => tr("Something went wrong. Try again later.").to_owned(),
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

/// The notes of the page: the one shown, and how many were said, so that a note's timer only
/// takes away its own note.
#[derive(Clone, Copy)]
pub struct Notices {
    shown: RwSignal<Option<String>>,
    said: StoredValue<u64>,
}

impl Notices {
    /// Give the components below a place to say notes, and return it.
    pub fn provide() -> Notices {
        let notices = Notices {
            shown: RwSignal::new(None),
            said: StoredValue::new(0),
        };
        provide_context(notices);
        notices
    }

    /// Say `text`, already in the page's language, for a few seconds.
    pub fn say(self, text: String) {
        let number = self.said.get_value().saturating_add(1);
        self.said.set_value(number);
        self.shown.set(Some(text));
        set_timeout(
            move || {
                if self.said.try_get_value() == Some(number) {
                    self.shown.set(None);
                }
            },
            SHOWN_FOR,
        );
    }

    /// Say `text` where the page has a place for notes.
    pub fn say_here(text: String) {
        if let Some(notices) = use_context::<Notices>() {
            notices.say(text);
        }
    }
}

/// The live region where notes appear.
#[component]
pub fn NoticeRegion(notices: Notices) -> impl IntoView {
    view! {
        <div class="notices" role="status" aria-live="polite" aria-atomic="true">
            {move || notices.shown.get().map(|text| view! { <p class="toast">{text}</p> })}
        </div>
    }
}
