//! The studio's Permissions tab, `/studio/permissions`: who runs a channel, as what, and whom it
//! invited, which its owners change. An owner invites an e-mail address as a role, and the person
//! says yes or no from their settings; owners change members' roles and take them off, and any
//! member, owners included while another stays, leaves.

use leptos::prelude::*;
use leptos_meta::Title;
use youcraft_api::commands::channel::Which;
use youcraft_api::commands::invite::EMAIL_LIMIT;
use youcraft_api::commands::member::{self, Invitation, Member, Members};
use youcraft_api::{AccountId, ChannelId, ChannelRole, routes};

use crate::account::SignedIn;
use crate::act::act;
use crate::app::titled;
use crate::browser;
use crate::call::call;
use crate::lang::tr;
use crate::pages::not_found::Trouble;
use crate::pages::studio::{StudioFrame, StudioTab, channel_role_name};

/// The Permissions tab.
#[component]
pub fn Permissions() -> impl IntoView {
    view! {
        <Title text=titled(tr("Permissions"))/>
        <main class="studio">
            <StudioFrame current=StudioTab::Permissions
                body=|me, membership| view! { <Loaded channel=membership.channel.id me=me.account/> }.into_any()/>
        </main>
    }
}

/// The members and invitations of `channel`, read again whenever one changes, for the person
/// `me`.
#[component]
fn Loaded(channel: ChannelId, me: AccountId) -> impl IntoView {
    let version = RwSignal::new(0_u32);
    let members = Resource::new(
        move || (channel, version.get()),
        |(channel, _)| call::<member::List>(Which { channel }),
    );
    view! {
        <InviteForm channel version/>
        <Suspense fallback=|| ()>
            {move || Suspend::new(async move {
                match members.await {
                    Ok(Members { members, invitations }) => {
                        let owners = members.iter().filter(|m| m.role == ChannelRole::Owner).count();
                        view! {
                            <section class="settings-section">
                                <h2>{tr("Members")}</h2>
                                <ul class="settings-list">
                                    {members.into_iter().map(|member| {
                                        let alone = member.role == ChannelRole::Owner && owners == 1;
                                        view! { <MemberRow channel member me alone version/> }
                                    }).collect_view()}
                                </ul>
                            </section>
                            {(!invitations.is_empty()).then(|| view! {
                                <section class="settings-section">
                                    <h2>{tr("Invitations")}</h2>
                                    <ul class="settings-list">
                                        {invitations.into_iter().map(|invitation| view! {
                                            <InvitationRow channel invitation version/>
                                        }).collect_view()}
                                    </ul>
                                </section>
                            })}
                        }.into_any()
                    }
                    Err(error) => view! { <Trouble error/> }.into_any(),
                }
            })}
        </Suspense>
    }
}

/// The roles a menu offers, the least first.
fn role_options(chosen: ChannelRole) -> impl IntoView {
    ChannelRole::ALL
        .map(|role| {
            view! {
                <option value=role.as_str() selected=role == chosen>{channel_role_name(role)}</option>
            }
        })
        .into_iter()
        .collect_view()
}

/// The form that invites an address as a role.
#[component]
fn InviteForm(channel: ChannelId, version: RwSignal<u32>) -> impl IntoView {
    let email = RwSignal::new(String::new());
    let role = RwSignal::new(ChannelRole::Editor);
    let invite = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        let invitation = member::NewInvitation {
            channel,
            email: email.get_untracked(),
            role: role.get_untracked(),
        };
        act::<member::Invite>(invitation, move |answer| {
            if answer.is_ok() {
                email.set(String::new());
                version.update(|v| *v += 1);
            }
        });
    };
    let choose = move |event: leptos::ev::Event| {
        if let Some(chosen) = ChannelRole::from_name(&event_target_value(&event)) {
            role.set(chosen);
        }
    };
    view! {
        <section class="settings-section">
            <h2>{tr("Invite someone")}</h2>
            <p class="settings-hint">{tr("They sign in with this address to say yes or no, within a week. Owners choose the members; managers also act as the channel; editors upload and change its videos; viewers see its studio.")}</p>
            <form class="settings-form" on:submit=invite>
                <input class="field" type="email" required aria-label=tr("E-mail address")
                    placeholder=tr("E-mail address") prop:value=email
                    on:input=move |event| email.set(browser::clipped(&event, EMAIL_LIMIT))/>
                <select class="field" aria-label=tr("Role") on:change=choose>
                    {role_options(ChannelRole::Editor)}
                </select>
                <button class="pill accent" type="submit">{tr("Invite")}</button>
            </form>
        </section>
    }
}

/// A member, with their role to change and the way off the channel; `alone` when they are its
/// only owner, whom it keeps.
#[component]
fn MemberRow(
    channel: ChannelId,
    member: Member,
    me: AccountId,
    alone: bool,
    version: RwSignal<u32>,
) -> impl IntoView {
    let account = member.account;
    let mine = account == me;
    let change = move |event: leptos::ev::Event| {
        if let Some(role) = ChannelRole::from_name(&event_target_value(&event)) {
            let wanted = member::NewRole {
                channel,
                account,
                role,
            };
            act::<member::SetRole>(wanted, move |_| {
                if mine {
                    // what they may do here changed with it
                    browser::reload();
                } else {
                    version.update(|v| *v += 1);
                }
            });
        }
    };
    let me_now = SignedIn::resource();
    let remove = move |_| {
        let seat = member::Seat { channel, account };
        act::<member::Remove>(seat, move |answer| {
            if answer.is_ok() && mine {
                // they left: the studio of this channel is no longer theirs
                browser::go(routes::STUDIO);
            } else {
                version.update(|v| *v += 1);
                if let Some(me) = me_now {
                    me.refetch();
                }
            }
        });
    };
    view! {
        <li class="settings-row admin-account">
            <div class="settings-row-text">
                <span>{member.name}</span>
                <bdi class="settings-hint">{member.email}</bdi>
            </div>
            <div class="admin-actions">
                <select class="field" aria-label=tr("Role") disabled=alone on:change=change>
                    {role_options(member.role)}
                </select>
                <button class="pill" type="button" disabled=alone on:click=remove>
                    {if mine { tr("Leave") } else { tr("Remove") }}
                </button>
            </div>
        </li>
    }
}

/// An invitation waiting, with the way to withdraw it.
#[component]
fn InvitationRow(
    channel: ChannelId,
    invitation: Invitation,
    version: RwSignal<u32>,
) -> impl IntoView {
    let email = invitation.email.clone();
    let withdraw = move |_| {
        let invitee = member::Invitee {
            channel,
            email: email.clone(),
        };
        act::<member::Uninvite>(invitee, move |_| version.update(|v| *v += 1));
    };
    view! {
        <li class="settings-row">
            <div class="settings-row-text">
                <bdi>{invitation.email}</bdi>
                <span class="settings-hint">{channel_role_name(invitation.role)}</span>
            </div>
            <button class="pill" type="button" on:click=withdraw>{tr("Withdraw")}</button>
        </li>
    }
}
