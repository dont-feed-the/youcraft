//! The settings page, `/account`: the signed-in person themselves, their account, their channels
//! and the invitations to join others (in `memberships`), appearance, language, the watch history
//! of the channel they act as, and personal tokens. How a channel shows itself, and who runs it,
//! is in the studio; what concerns the site, in the administration.
//!
//! A role is shown only above what the site grants every account.

use leptos::prelude::*;
use leptos_meta::Title;
use youcraft_api::commands::account::{Me, Theme};
use youcraft_api::commands::history::{self, Pausing};
use youcraft_api::commands::site::{GetPolicy, Nothing};
use youcraft_api::commands::token::{self, Token};
use youcraft_api::{ApiError, Grant, routes};

use crate::account::{Shown, SignedIn, choose_language, choose_theme};
use crate::act::act;
use crate::app::titled;
use crate::browser;
use crate::call::call;
use crate::components::avatar::{Face, Size};
use crate::icons::{Glyph, Icon};
use crate::lang::{Language, RenderTime, fill, offered, tr};
use crate::pages::admin::role_name;
use crate::pages::memberships::{ChannelsSection, InvitationsSection};
use crate::pages::not_found::Trouble;

/// The settings page.
#[component]
pub fn Account() -> impl IntoView {
    let me = SignedIn::resource();
    let policy = Resource::new(|| (), |()| call::<GetPolicy>(Nothing {}));
    view! {
        <Title text=titled(tr("Settings"))/>
        <main class="settings">
            <h1 class="settings-title">{tr("Settings")}</h1>
            <Suspense fallback=|| ()>
                {move || Suspend::new(async move {
                    let me = match me {
                        Some(me) => me.await,
                        None => None,
                    };
                    // Without the policy, nothing counts as granted to every account.
                    let floor = policy.await.ok().map(|policy| policy.signed_in_role);
                    match me {
                        Some(me) => view! { <Sections me floor/> }.into_any(),
                        None => view! { <Trouble error=ApiError::SignInRequired/> }.into_any(),
                    }
                })}
            </Suspense>
        </main>
    }
}

/// Every section, for `me`, on a site that grants every account `floor`, if it is known.
#[component]
fn Sections(me: Me, floor: Option<Grant>) -> impl IntoView {
    let paused = me.history_paused;
    let acting = me.channel.clone();
    let blocked = me.blocked.then(|| {
        view! { <p class="settings-notice">{tr("Your account is blocked: you can watch videos, but not like, comment or publish.")}</p> }
    });
    let channels = me.clone();
    let me = StoredValue::new(me);
    let face = acting.clone().map(|channel| {
        view! {
            <span class="avatar avatar-large"><Face channel size=Size::Large/></span>
        }
    });
    let link = acting.map(|channel| {
        view! {
            <span class="settings-hint"><bdi>{format!("@{}", channel.handle)}</bdi></span>
            <a class="menu-link" href=routes::channel(&channel.handle)>{tr("View your channel")}</a>
        }
    });
    view! {
        <section class="settings-section">
            <h2>{tr("Account")}</h2>
            <div class="settings-account">
                {face}
                <div class="settings-account-names">
                    <span class="settings-account-name">{me.with_value(|me| me.name.clone())}</span>
                    {me.with_value(|me| (Some(me.role) > floor).then(|| view! {
                        <span class="settings-hint">{role_name(me.role)}</span>
                    }))}
                    {link}
                </div>
            </div>
            {blocked}
        </section>
        <InvitationsSection/>
        <ChannelsSection me=channels/>
        <AppearanceSection me/>
        <LanguageSection me/>
        {me.with_value(|me| me.channel.is_some()).then(|| view! { <HistorySection paused/> })}
        <TokenSection/>
    }
}

#[component]
fn AppearanceSection(me: StoredValue<Me>) -> impl IntoView {
    let shown = use_context::<Shown>().map(|Shown(shown)| shown);
    let choices = Theme::ALL.map(|theme| {
        let label = match theme {
            Theme::Device => tr("Use device theme"),
            Theme::Dark => tr("Dark theme"),
            Theme::Light => tr("Light theme"),
        };
        let chosen = move || shown.is_some_and(|shown| shown.get() == theme);
        view! {
            <button class="choice" type="button" class:chosen=chosen
                aria-pressed=move || if chosen() { "true" } else { "false" }
                on:click=move |_| me.with_value(|me| choose_theme(theme, Some(me)))>
                {label}
            </button>
        }
    });
    view! {
        <section class="settings-section">
            <h2>{tr("Appearance")}</h2>
            <div class="choices">{choices.into_iter().collect_view()}</div>
        </section>
    }
}

#[component]
fn LanguageSection(me: StoredValue<Me>) -> impl IntoView {
    let current = youcraft_i18n::code(Language::current());
    let options = offered().into_iter().map(|(code, name)| {
        let selected = code == current;
        view! { <option value=code.clone() lang=code selected=selected>{name}</option> }
    });
    let change = move |event: leptos::ev::Event| {
        let code = event_target_value(&event);
        me.with_value(|me| choose_language(code, Some(me)));
    };
    view! {
        <section class="settings-section">
            <h2>{tr("Language")}</h2>
            <select class="field" aria-label=tr("Language") on:change=change>
                {options.collect_view()}
            </select>
        </section>
    }
}

/// Whether the watch history keeps what the person watches, as its own page also sets.
#[component]
fn HistorySection(paused: bool) -> impl IntoView {
    let paused = RwSignal::new(paused);
    let toggle = move |_| {
        let wanted = !paused.get_untracked();
        act::<history::Pause>(Pausing { paused: wanted }, move |answer| {
            if let Ok(now) = answer {
                paused.set(now.paused);
            }
        });
    };
    view! {
        <section class="settings-section">
            <h2>{tr("Watch history")}</h2>
            <Show when=move || paused.get()>
                <p class="settings-hint">{tr("Your history is paused: what you watch now is not kept.")}</p>
            </Show>
            <div class="choices">
                <button class="pill" type="button" aria-pressed=move || if paused.get() { "true" } else { "false" } on:click=toggle>
                    {move || if paused.get() { tr("Turn on watch history") } else { tr("Pause watch history") }}
                </button>
            </div>
        </section>
    }
}

/// The personal tokens: made here, listed with their use, revoked here.
#[component]
fn TokenSection() -> impl IntoView {
    let version = RwSignal::new(0_u32);
    let tokens = Resource::new(move || version.get(), |_| call::<token::List>(Nothing {}));
    let name = RwSignal::new(String::new());
    let secret = RwSignal::new(None::<String>);
    let create = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        let naming = token::Naming {
            name: name.get_untracked(),
        };
        act::<token::Create>(naming, move |answer| {
            if let Ok(created) = answer {
                secret.set(Some(created.secret));
                name.set(String::new());
                version.update(|v| *v += 1);
            }
        });
    };
    view! {
        <section class="settings-section">
            <h2>{tr("Personal tokens")}</h2>
            <p class="settings-hint">{tr("A personal token lets a program act as you, with all your rights. Keep it secret.")}</p>
            <form class="settings-form" on:submit=create>
                <input class="field" type="text" required aria-label=tr("Token name")
                    placeholder=tr("Token name") prop:value=name
                    on:input=move |event| name.set(browser::clipped(&event, token::NAME_LIMIT))/>
                <button class="pill" type="submit">{tr("Create token")}</button>
            </form>
            {move || secret.get().map(|secret| view! {
                <div class="settings-secret">
                    <p>{tr("Copy this token now: it will not be shown again.")}</p>
                    <input class="field secret" type="text" readonly value=secret
                        on:focus=move |event| select_all(&event)/>
                </div>
            })}
            <Suspense fallback=|| ()>
                {move || Suspend::new(async move {
                    let tokens = tokens.await.unwrap_or_default();
                    view! {
                        <ul class="settings-list">
                            {tokens.into_iter().map(|token| view! { <TokenRow token version/> }).collect_view()}
                        </ul>
                    }
                })}
            </Suspense>
        </section>
    }
}

#[component]
fn TokenRow(token: Token, version: RwSignal<u32>) -> impl IntoView {
    let now = RenderTime::current();
    let lang = Language::current();
    let created = fill(
        tr("Created {when}"),
        &[(
            "when",
            &youcraft_i18n::when::ago(lang, token.created_at, now),
        )],
    );
    let used = token.last_used_at.map(|used| {
        fill(
            tr("Last used {when}"),
            &[("when", &youcraft_i18n::when::ago(lang, used, now))],
        )
    });
    let id = token.id;
    let revoke = move |_| {
        act::<token::Revoke>(token::Which { token: id }, move |_| {
            version.update(|v| *v += 1);
        });
    };
    view! {
        <li class="settings-row">
            <Glyph icon=Icon::Key/>
            <div class="settings-row-text">
                <span>{token.name}</span>
                <span class="settings-hint">{created}{used.map(|used| format!(" · {used}"))}</span>
            </div>
            <button class="pill" type="button" on:click=revoke>{tr("Revoke")}</button>
        </li>
    }
}

/// Select the whole text of the field an event came from, so the secret is easy to copy.
fn select_all(event: &leptos::ev::FocusEvent) {
    #[cfg(all(feature = "hydrate", not(feature = "ssr")))]
    {
        use wasm_bindgen::JsCast;
        if let Some(field) = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
        {
            field.select();
        }
    }
    #[cfg(feature = "ssr")]
    let _ = event;
}
