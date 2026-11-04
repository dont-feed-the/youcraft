//! The administration, `/admin`, for administrators: the invitations waiting, and every account
//! of the site, found by name or handle, with what administrators do to one: change its role,
//! block or unblock it, take its photo away or name it after its handle again. Accounts are
//! shown by their channel and never by their e-mail address.

use leptos::prelude::*;
use leptos_meta::Title;
use youcraft_api::commands::admin::{self, AccountRow, Finding};
use youcraft_api::commands::channel::Handle;
use youcraft_api::commands::invite::{self, Invitation};
use youcraft_api::commands::search::QUERY_LIMIT;
use youcraft_api::commands::site::{GetPolicy, Nothing};
use youcraft_api::{ApiError, ChannelId, Grant, routes};

use crate::account::SignedIn;
use crate::act::{Optimistic, act};
use crate::app::titled;
use crate::browser;
use crate::call::call;
use crate::components::avatar::{Face, Size};
use crate::components::more::{Pages, ShowMore};
use crate::icons::{Glyph, Icon};
use crate::lang::tr;
use crate::pages::not_found::Trouble;

/// The name of `role`.
pub fn role_name(role: Grant) -> &'static str {
    match role {
        Grant::Viewer => tr("Viewer"),
        Grant::Creator => tr("Creator"),
        Grant::Admin => tr("Administrator"),
    }
}

/// The administration page.
#[component]
pub fn Admin() -> impl IntoView {
    let me = SignedIn::resource();
    let policy = Resource::new(|| (), |()| call::<GetPolicy>(Nothing {}));
    view! {
        <Title text=titled(tr("Administration"))/>
        <main class="settings">
            <h1 class="settings-title">{tr("Administration")}</h1>
            <Suspense fallback=|| ()>
                {move || Suspend::new(async move {
                    let me = match me {
                        Some(me) => me.await,
                        None => None,
                    };
                    // Without the policy, nothing counts as granted to every account.
                    let floor = policy.await.ok().map(|policy| policy.signed_in_role);
                    match me {
                        Some(me) if me.role >= Grant::Admin => view! {
                            <InvitationSection floor/>
                            <AccountsSection own=me.channel.id/>
                        }.into_any(),
                        Some(_) => view! { <Trouble error=ApiError::Forbidden/> }.into_any(),
                        None => view! { <Trouble error=ApiError::SignInRequired/> }.into_any(),
                    }
                })}
            </Suspense>
        </main>
    }
}

/// The invitations waiting: made and withdrawn here, to the roles above `floor` and above a
/// viewer's, which needs no invitation.
#[component]
fn InvitationSection(floor: Option<Grant>) -> impl IntoView {
    let offered: Vec<Grant> = Grant::ALL
        .into_iter()
        .filter(|role| Some(*role) > floor.max(Some(Grant::Viewer)))
        .collect();
    let first = offered.first().copied().unwrap_or(Grant::Admin);
    let hint = if first == Grant::Admin {
        tr("Invited people become administrators when they first sign in with Google.")
    } else {
        tr("Invited people become creators or administrators when they first sign in with Google.")
    };
    let version = RwSignal::new(0_u32);
    let waiting = Resource::new(move || version.get(), |_| call::<invite::List>(Nothing {}));
    let email = RwSignal::new(String::new());
    let role = RwSignal::new(first);
    let send = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        let invitation = invite::NewInvitation {
            email: email.get_untracked(),
            role: role.get_untracked(),
        };
        act::<invite::Create>(invitation, move |sent| {
            if sent.is_ok() {
                email.set(String::new());
            }
            version.update(|v| *v += 1);
        });
    };
    let pick = move |event: leptos::ev::Event| {
        if let Some(picked) = Grant::from_name(&event_target_value(&event)) {
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
        act::<invite::Revoke>(address.clone(), move |_| {
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

/// Every account, the newest first, found by the words of the search.
#[component]
fn AccountsSection(own: ChannelId) -> impl IntoView {
    let query = RwSignal::new(String::new());
    let searched = RwSignal::new(String::new());
    let first = Resource::new(
        move || searched.get(),
        |query| call::<admin::Accounts>(Finding { query, after: None }),
    );
    let search = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        searched.set(query.get_untracked());
    };
    view! {
        <section class="settings-section">
            <h2>{tr("Accounts")}</h2>
            <form class="settings-form" role="search" on:submit=search>
                <input class="field" type="search" aria-label=tr("Search accounts")
                    placeholder=tr("Search accounts") prop:value=query
                    on:input=move |event| query.set(browser::clipped(&event, QUERY_LIMIT))/>
                <button class="pill" type="submit">{tr("Search")}</button>
            </form>
            <Suspense fallback=|| ()>
                {move || Suspend::new(async move {
                    match first.await {
                        Ok(page) => {
                            let pages: Pages<AccountRow> = Pages::new(page.accounts, page.next);
                            let more = Callback::new(move |()| {
                                let query = searched.get_untracked();
                                pages.read_next::<admin::Accounts>(
                                    |after| Finding { query, after },
                                    |page| (page.accounts, page.next),
                                );
                            });
                            view! {
                                <ul class="settings-list">
                                    <For each=move || pages.items.get() key=|row| row.channel.id let:row>
                                        <AccountEntry own=row.channel.id == own row/>
                                    </For>
                                </ul>
                                <ShowMore pages more/>
                            }.into_any()
                        }
                        Err(error) => view! { <Trouble error/> }.into_any(),
                    }
                })}
            </Suspense>
        </section>
    }
}

/// One account and what an administrator does to it; `own` when it is theirs, whose role and
/// blocking they cannot change. Its role and blocking show the change at once, and what the site
/// last confirmed if the change fails.
#[component]
fn AccountEntry(row: AccountRow, own: bool) -> impl IntoView {
    let role = Optimistic::new(row.role);
    let blocked = Optimistic::new(row.blocked);
    let channel = RwSignal::new(row.channel);
    let handle = move || channel.with(|channel| channel.handle.clone());
    let pick = move |event: leptos::ev::Event| {
        let Some(wanted) = Grant::from_name(&event_target_value(&event)) else {
            return;
        };
        let change = admin::NewRole {
            handle: handle(),
            role: wanted,
        };
        role.change::<admin::SetRole>(wanted, change, move |_| wanted);
    };
    let block = move |_| {
        let wanted = !blocked.shown.get_untracked();
        let change = admin::Blocking {
            handle: handle(),
            blocked: wanted,
        };
        blocked.change::<admin::Block>(wanted, change, move |_| wanted);
    };
    let remove_photo = move |_| {
        act::<admin::RemovePhoto>(Handle { handle: handle() }, move |answer| {
            if let Ok(profile) = answer {
                channel.set(profile.card);
            }
        });
    };
    let reset_name = move |_| {
        act::<admin::ResetName>(Handle { handle: handle() }, move |answer| {
            if let Ok(profile) = answer {
                channel.set(profile.card);
            }
        });
    };
    view! {
        <li class="settings-row admin-account">
            <a class="avatar" href=move || routes::channel(&handle()) aria-hidden="true" tabindex="-1">
                {move || view! { <Face channel=channel.get() size=Size::Regular/> }}
            </a>
            <div class="settings-row-text">
                <a href=move || routes::channel(&handle()) dir="auto">{move || channel.with(|channel| channel.name.clone())}</a>
                <span class="settings-hint">
                    <bdi>{move || format!("@{}", handle())}</bdi>
                    {move || blocked.shown.get().then(|| format!(" · {}", tr("Blocked")))}
                </span>
            </div>
            <div class="admin-actions">
                {(!own).then(|| view! {
                    <select class="field" aria-label=tr("Role") on:change=pick>
                        {Grant::ALL.map(|choice| view! {
                            <option value=choice.as_str() selected=role.shown.get_untracked() == choice
                                prop:selected=move || role.shown.get() == choice>
                                {role_name(choice)}
                            </option>
                        }).collect_view()}
                    </select>
                    <button class="pill" type="button" on:click=block>
                        {move || if blocked.shown.get() { tr("Unblock") } else { tr("Block") }}
                    </button>
                })}
                <Show when=move || channel.with(|channel| channel.photo.is_some())>
                    <button class="pill" type="button" on:click=remove_photo>{tr("Remove picture")}</button>
                </Show>
                <Show when=move || channel.with(|channel| channel.name != channel.handle)>
                    <button class="pill" type="button" on:click=reset_name>{tr("Reset name")}</button>
                </Show>
            </div>
        </li>
    }
}
