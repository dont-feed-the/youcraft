//! The studio's Customization tab, `/studio/customize`: how a channel shows itself, its photo,
//! name and description, which its owners change. The handle stays, so no link to the channel
//! ever breaks.

use leptos::prelude::*;
use leptos_meta::Title;
use youcraft_api::commands::channel::{self, ChannelPage, NAME_LIMIT, Which};
use youcraft_api::commands::profile::{self, Customization, DESCRIPTION_LIMIT, Profile};
use youcraft_api::records::ChannelCard;
use youcraft_api::routes;

use crate::account::SignedIn;
use crate::act::{Scope, act};
use crate::app::titled;
use crate::browser;
use crate::call::call;
use crate::components::avatar::{Face, Size};
use crate::lang::tr;
use crate::notice::Notices;
use crate::pages::not_found::Trouble;
use crate::pages::studio::{StudioFrame, StudioTab};

/// The Customization tab.
#[component]
pub fn Customize() -> impl IntoView {
    view! {
        <Title text=titled(tr("Customization"))/>
        <main class="studio">
            <StudioFrame current=StudioTab::Customization
                body=|_, membership| view! { <Loaded handle=membership.channel.handle/> }.into_any()/>
        </main>
    }
}

/// The forms, once the channel of `handle` has come.
#[component]
fn Loaded(handle: String) -> impl IntoView {
    let page = Resource::new(
        move || handle.clone(),
        |handle| call::<channel::Get>(channel::Handle { handle }),
    );
    view! {
        <Suspense fallback=|| ()>
            {move || Suspend::new(async move {
                match page.await {
                    Ok(page) => view! { <Forms page/> }.into_any(),
                    Err(error) => view! { <Trouble error/> }.into_any(),
                }
            })}
        </Suspense>
    }
}

/// The photo, then the name and description; what each change did is said in the page's
/// live region.
#[component]
fn Forms(page: ChannelPage) -> impl IntoView {
    let card = RwSignal::new(page.card.clone());
    let name = RwSignal::new(page.card.name.clone());
    let description = RwSignal::new(page.description.clone());
    let busy = RwSignal::new(false);
    // read here, where the component's owner is, since the answers come back outside it: what
    // the masthead shows changes with the channel, and the notes go to the site's live region
    let me = SignedIn::resource();
    let notices = use_context::<Notices>();
    let shown = move |profile: Profile| {
        card.set(profile.card);
        if let Some(me) = me {
            me.refetch();
        }
    };
    let saved = move || {
        if let Some(notices) = notices {
            notices.say(tr("Saved.").to_owned());
        }
    };
    let channel = page.card.id;
    let save = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        let change = Customization {
            channel,
            name: Some(name.get_untracked()),
            description: Some(description.get_untracked()),
        };
        busy.set(true);
        act::<profile::Update>(change, move |answer| {
            busy.set(false);
            if let Ok(profile) = answer {
                name.set(profile.card.name.clone());
                description.set(profile.description.clone());
                shown(profile);
                saved();
            }
        });
    };
    let scope = Scope::current();
    let choose = move |event: leptos::ev::Event| {
        #[cfg(feature = "hydrate")]
        if let Some(file) = crate::transfer::chosen_file(&event) {
            busy.set(true);
            let scope = scope.clone();
            leptos::task::spawn_local(async move {
                let image = crate::transfer::base64_of(&file, profile::PHOTO_BYTES).await;
                scope.within(move || match image {
                    Ok(image) => act::<profile::SetPhoto>(
                        profile::NewPhoto { channel, image },
                        move |sent| {
                            busy.set(false);
                            if let Ok(profile) = sent {
                                shown(profile);
                                saved();
                            }
                        },
                    ),
                    Err(error) => {
                        busy.set(false);
                        Notices::say_here(crate::notice::describe(&error));
                    }
                });
            });
        }
        #[cfg(not(feature = "hydrate"))]
        let _ = (event, &scope);
    };
    let remove = move |_| {
        busy.set(true);
        act::<profile::RemovePhoto>(Which { channel }, move |answer| {
            busy.set(false);
            if let Ok(profile) = answer {
                shown(profile);
            }
        });
    };
    let face = move || {
        let channel: ChannelCard = card.get();
        view! { <Face channel size=Size::Large/> }
    };
    view! {
        <section class="settings-section">
            <h2>{tr("Picture")}</h2>
            <p class="settings-hint">{tr("Your picture shows beside your channel and your comments. Choose a JPEG, PNG or WebP file of at most 5 MB; its centre is cut square.")}</p>
            <div class="picture-choice">
                <span class="avatar avatar-large" aria-hidden="true">{face}</span>
                <label class="pill picker">
                    <span>{tr("Select file")}</span>
                    <input class="file" type="file" accept="image/jpeg,image/png,image/webp"
                        disabled=move || busy.get() on:change=choose/>
                </label>
                <Show when=move || card.with(|card| card.photo.is_some())>
                    <button class="pill" type="button" disabled=move || busy.get() on:click=remove>
                        {tr("Remove")}
                    </button>
                </Show>
            </div>
        </section>
        <form class="editor customize-form" on:submit=save>
            <label class="labelled">
                <span>{tr("Name")}</span>
                <input class="field" type="text" dir="auto" required prop:value=name
                    on:input=move |event| name.set(browser::clipped(&event, NAME_LIMIT))/>
            </label>
            <p class="settings-hint"><bdi>{routes::channel(&page.card.handle)}</bdi></p>
            <label class="labelled">
                <span>{tr("Description")}</span>
                <textarea class="field" rows="5" dir="auto" prop:value=description
                    on:input=move |event| description.set(browser::clipped(&event, DESCRIPTION_LIMIT))></textarea>
            </label>
            <div class="editor-actions">
                <button class="pill accent" type="submit" disabled=move || busy.get()>{tr("Save")}</button>
            </div>
        </form>
        <DeleteSection channel/>
    }
}

/// The way to delete the channel, with all that is its own, for good.
#[component]
fn DeleteSection(channel: youcraft_api::ChannelId) -> impl IntoView {
    let delete = move |_| {
        #[cfg(feature = "hydrate")]
        if crate::transfer::confirm(tr(
            "Delete this channel, with its videos, comments and playlists, for good?",
        )) {
            act::<channel::Delete>(Which { channel }, |answer| {
                if answer.is_ok() {
                    browser::go(routes::ACCOUNT_CHANNELS);
                }
            });
        }
        #[cfg(not(feature = "hydrate"))]
        let _ = channel;
    };
    view! {
        <section class="settings-section">
            <h2>{tr("Delete channel")}</h2>
            <p class="settings-hint">{tr("Its videos, comments and playlists go with it, for good.")}</p>
            <div class="choices">
                <button class="pill danger" type="button" on:click=delete>{tr("Delete channel")}</button>
            </div>
        </section>
    }
}
