//! The channels of the person signed in, on their settings page: those they belong to, with
//! their role, the way to act as one they manage, to its studio and off it; the form that makes a
//! new one; and the invitations to join others, which they accept or decline.

use leptos::prelude::*;
use youcraft_api::commands::account::{self, Acting, Me, Membership};
use youcraft_api::commands::channel::{self, NAME_LIMIT, Which};
use youcraft_api::commands::member::{self, Offer};
use youcraft_api::commands::site::Nothing;
use youcraft_api::{ChannelId, ChannelRole, routes};

use crate::act::act;
use crate::browser;
use crate::call::call;
use crate::components::avatar::{Face, Size};
use crate::lang::{fill, tr};
use crate::pages::studio::channel_role_name;

/// The channels `me` belongs to, and the way to make another.
#[component]
pub fn ChannelsSection(me: Me) -> impl IntoView {
    let acting = me.channel.as_ref().map(|card| card.id);
    let account = me.account;
    let name = RwSignal::new(String::new());
    let create = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        let new = channel::NewChannel {
            name: name.get_untracked(),
        };
        act::<channel::Create>(new, move |answer| {
            if let Ok(card) = answer {
                // they act as it now: everything shown changes with it
                browser::go(&routes::channel(&card.handle));
            }
        });
    };
    let rows = me
        .channels
        .into_iter()
        .map(|membership| {
            let current = Some(membership.channel.id) == acting;
            view! { <ChannelRow membership current account/> }
        })
        .collect_view();
    view! {
        <section class="settings-section" id="channels">
            <h2>{tr("Your channels")}</h2>
            <p class="settings-hint">{tr("What you do on YouCraft, you do as one of your channels: it comments, likes, subscribes and keeps playlists. Several people may run one channel.")}</p>
            <ul class="settings-list">{rows}</ul>
            <form class="settings-form" on:submit=create>
                <input class="field" type="text" required aria-label=tr("Channel name")
                    placeholder=tr("Channel name") prop:value=name
                    on:input=move |event| name.set(browser::clipped(&event, NAME_LIMIT))/>
                <button class="pill" type="submit">{tr("Create a channel")}</button>
            </form>
        </section>
    }
}

/// A channel the person belongs to: `current` when they act as it.
#[component]
fn ChannelRow(
    membership: Membership,
    current: bool,
    account: youcraft_api::AccountId,
) -> impl IntoView {
    let channel = membership.channel.id;
    let speaks = membership.role.allows(ChannelRole::Manager);
    let switch = move |_| act_as(channel);
    let leave = move |_| {
        #[cfg(feature = "hydrate")]
        if crate::transfer::confirm(tr("Leave this channel?")) {
            let seat = member::Seat { channel, account };
            act::<member::Remove>(seat, |answer| {
                if answer.is_ok() {
                    browser::reload();
                }
            });
        }
        #[cfg(not(feature = "hydrate"))]
        let _ = (channel, account);
    };
    view! {
        <li class="settings-row admin-account">
            <span class="avatar" aria-hidden="true"><Face channel=membership.channel.clone() size=Size::Regular/></span>
            <div class="settings-row-text">
                <a href=routes::channel(&membership.channel.handle)>{membership.channel.name.clone()}</a>
                <span class="settings-hint">
                    <bdi>{format!("@{}", membership.channel.handle)}</bdi>
                    {format!(" · {}", channel_role_name(membership.role))}
                    {current.then(|| format!(" · {}", tr("You act as this channel")))}
                </span>
            </div>
            <div class="admin-actions">
                {(speaks && !current).then(|| view! {
                    <button class="pill" type="button" on:click=switch>{tr("Switch to this channel")}</button>
                })}
                <a class="pill" href=routes::in_studio(routes::STUDIO, channel)>{tr("YouCraft Studio")}</a>
                <button class="pill" type="button" on:click=leave>{tr("Leave")}</button>
            </div>
        </li>
    }
}

/// Act as `channel` from now on, then draw the page again as it.
pub fn act_as(channel: ChannelId) {
    act::<account::ActAs>(Acting { channel }, |answer| {
        if answer.is_ok() {
            browser::reload();
        }
    });
}

/// The invitations that wait for the person's answer; nothing when none does.
#[component]
pub fn InvitationsSection() -> impl IntoView {
    let version = RwSignal::new(0_u32);
    let offers = Resource::new(
        move || version.get(),
        |_| call::<member::Offers>(Nothing {}),
    );
    view! {
        <Suspense fallback=|| ()>
            {move || Suspend::new(async move {
                let offers = offers.await.unwrap_or_default();
                (!offers.is_empty()).then(|| view! {
                    <section class="settings-section" id="invitations">
                        <h2>{tr("Invitations")}</h2>
                        <ul class="settings-list">
                            {offers.into_iter().map(|offer| view! { <OfferRow offer version/> }).collect_view()}
                        </ul>
                    </section>
                })
            })}
        </Suspense>
    }
}

/// An invitation, with the ways to answer it.
#[component]
fn OfferRow(offer: Offer, version: RwSignal<u32>) -> impl IntoView {
    let channel = offer.channel.id;
    let accept = move |_| {
        act::<member::Accept>(Which { channel }, |answer| {
            if answer.is_ok() {
                browser::reload();
            }
        });
    };
    let decline = move |_| {
        act::<member::Decline>(Which { channel }, move |_| version.update(|v| *v += 1));
    };
    let text = fill(
        tr("{channel} invites you to join as {role}."),
        &[
            ("channel", &offer.channel.name),
            ("role", channel_role_name(offer.role)),
        ],
    );
    view! {
        <li class="settings-row admin-account">
            <span class="avatar" aria-hidden="true"><Face channel=offer.channel.clone() size=Size::Regular/></span>
            <div class="settings-row-text">
                <span>{text}</span>
                <bdi class="settings-hint">{format!("@{}", offer.channel.handle)}</bdi>
            </div>
            <div class="admin-actions">
                <button class="pill" type="button" on:click=decline>{tr("Decline")}</button>
                <button class="pill accent" type="button" on:click=accept>{tr("Accept")}</button>
            </div>
        </li>
    }
}
