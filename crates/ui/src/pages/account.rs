//! The settings page, `/account`: the signed-in person's account, appearance, language and
//! personal tokens, and for administrators the invitations waiting.
//!
//! A role is shown, and offered to invite with, only above what the site grants every account.

use leptos::prelude::*;
use leptos_meta::Title;
use youcraft_api::commands::account::{Me, Theme};
use youcraft_api::commands::invite::{self, Invitation};
use youcraft_api::commands::site::{GetPolicy, Nothing};
use youcraft_api::commands::token::{self, Token};
use youcraft_api::{ApiError, Role, routes};

use crate::account::{Shown, SignedIn, choose_language, choose_theme};
use crate::call::call;
use crate::components::account_menu::initial;
use crate::icons::{Glyph, Icon};
use crate::lang::{Language, RenderTime, fill, offered, tr};
use crate::pages::not_found::Trouble;

/// The settings page.
#[component]
pub fn Account() -> impl IntoView {
    let me = SignedIn::resource();
    let policy = Resource::new(|| (), |()| call::<GetPolicy>(Nothing {}));
    view! {
        <Title text=format!("{} - YouCraft", tr("Settings"))/>
        <main class="settings">
            <h1 class="settings-title">{tr("Settings")}</h1>
            <Suspense fallback=|| ()>
                {move || Suspend::new(async move {
                    let me = match me {
                        Some(me) => me.await,
                        None => None,
                    };
                    // Without the policy, every role counts as above the least one.
                    let floor = policy.await.map_or(Role::Anyone, |policy| policy.signed_in_role);
                    match me {
                        Some(me) => view! { <Sections me floor/> }.into_any(),
                        None => view! { <Trouble error=ApiError::SignInRequired/> }.into_any(),
                    }
                })}
            </Suspense>
        </main>
    }
}

/// The name of `role`.
fn role_name(role: Role) -> &'static str {
    match role {
        Role::Anyone | Role::Viewer => tr("Viewer"),
        Role::Creator => tr("Creator"),
        Role::Admin => tr("Administrator"),
    }
}

/// Every section, for `me`, on a site where every account has at least the role `floor`.
#[component]
fn Sections(me: Me, floor: Role) -> impl IntoView {
    let admin = me.role >= Role::Admin;
    let channel = routes::channel(&me.channel.handle);
    let blocked = me.blocked.then(|| {
        view! { <p class="settings-notice">{tr("Your account is blocked: you can watch videos, but not like, comment or publish.")}</p> }
    });
    let me = StoredValue::new(me);
    view! {
        <section class="settings-section">
            <h2>{tr("Account")}</h2>
            <div class="settings-account">
                <span class="avatar avatar-large">{me.with_value(|me| initial(&me.channel.name))}</span>
                <div class="settings-account-names">
                    <span class="settings-account-name">{me.with_value(|me| me.channel.name.clone())}</span>
                    <span class="settings-hint">
                        <bdi>{me.with_value(|me| format!("@{}", me.channel.handle))}</bdi>
                        {me.with_value(|me| (me.role > floor).then(|| format!(" · {}", role_name(me.role))))}
                    </span>
                    <a class="menu-link" href=channel>{tr("View your channel")}</a>
                </div>
            </div>
            {blocked}
        </section>
        <AppearanceSection me/>
        <LanguageSection me/>
        <TokenSection/>
        {admin.then(|| view! { <InvitationSection floor/> })}
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
        leptos::task::spawn_local(async move {
            if let Ok(created) = call::<token::Create>(naming).await {
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
                <input class="field" type="text" maxlength="100" required aria-label=tr("Token name")
                    placeholder=tr("Token name") prop:value=name
                    on:input=move |event| name.set(event_target_value(&event))/>
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
        leptos::task::spawn_local(async move {
            let _ = call::<token::Revoke>(token::Which { token: id }).await;
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

/// The invitations waiting, for administrators: made and withdrawn here, to the roles above
/// `floor`.
#[component]
fn InvitationSection(floor: Role) -> impl IntoView {
    let offered: Vec<Role> = [Role::Creator, Role::Admin]
        .into_iter()
        .filter(|role| *role > floor)
        .collect();
    let first = offered.first().copied().unwrap_or(Role::Admin);
    let hint = if first == Role::Admin {
        tr("Invited people become administrators when they first sign in with Google.")
    } else {
        tr("Invited people become creators or administrators when they first sign in with Google.")
    };
    let version = RwSignal::new(0_u32);
    let waiting = Resource::new(move || version.get(), |_| call::<invite::List>(Nothing {}));
    let email = RwSignal::new(String::new());
    let role = RwSignal::new(first);
    let failed = RwSignal::new(false);
    let send = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        let invitation = invite::NewInvitation {
            email: email.get_untracked(),
            role: role.get_untracked(),
        };
        leptos::task::spawn_local(async move {
            let sent = call::<invite::Create>(invitation).await;
            failed.set(sent.is_err());
            if sent.is_ok() {
                email.set(String::new());
            }
            version.update(|v| *v += 1);
        });
    };
    let pick = move |event: leptos::ev::Event| {
        if let Some(picked) = Role::from_name(&event_target_value(&event)) {
            role.set(picked);
        }
    };
    // One role to give needs no choice.
    let choice = (offered.len() > 1).then(|| {
        view! {
            <select class="field" aria-label=tr("Role") on:change=pick>
                {offered.iter().map(|role| view! { <option value=role.as_str()>{role_name(*role)}</option> }).collect_view()}
            </select>
        }
    });
    view! {
        <section class="settings-section">
            <h2>{tr("Invitations")}</h2>
            <p class="settings-hint">{hint}</p>
            <form class="settings-form" on:submit=send>
                <input class="field" type="email" required aria-label=tr("E-mail address")
                    placeholder=tr("E-mail address") prop:value=email
                    on:input=move |event| email.set(event_target_value(&event))/>
                {choice}
                <button class="pill" type="submit">{tr("Invite")}</button>
            </form>
            {move || failed.get().then(|| view! { <p class="settings-notice">{tr("Something went wrong. Try again later.")}</p> })}
            <Suspense fallback=|| ()>
                {move || Suspend::new(async move {
                    let waiting = waiting.await.unwrap_or_default();
                    view! {
                        <ul class="settings-list">
                            {waiting.into_iter().map(|invitation| view! { <InvitationRow invitation version/> }).collect_view()}
                        </ul>
                    }
                })}
            </Suspense>
        </section>
    }
}

#[component]
fn InvitationRow(invitation: Invitation, version: RwSignal<u32>) -> impl IntoView {
    let address = invite::Address {
        email: invitation.email.clone(),
    };
    let withdraw = move |_| {
        let address = address.clone();
        leptos::task::spawn_local(async move {
            let _ = call::<invite::Revoke>(address).await;
            version.update(|v| *v += 1);
        });
    };
    view! {
        <li class="settings-row">
            <Glyph icon=Icon::Mail/>
            <div class="settings-row-text">
                <span>{invitation.email}</span>
                <span class="settings-hint">{role_name(invitation.role)}</span>
            </div>
            <button class="pill" type="button" on:click=withdraw>{tr("Withdraw")}</button>
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
