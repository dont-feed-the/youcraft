//! The studio's Customization tab, `/studio/customize`: how the caller's channel shows itself,
//! its photo, name and description, which every signed-in account changes on its own channel.
//! The handle stays, so no link to the channel ever breaks.

use leptos::prelude::*;
use leptos_meta::Title;
use youcraft_api::commands::channel::{self, ChannelPage, NAME_LIMIT};
use youcraft_api::commands::profile::{self, Customization, DESCRIPTION_LIMIT, Profile};
use youcraft_api::commands::site::Nothing;
use youcraft_api::records::ChannelCard;
use youcraft_api::{ApiError, routes};

use crate::account::SignedIn;
use crate::browser;
use crate::call::call;
use crate::components::avatar::{Face, Size};
use crate::lang::tr;
use crate::pages::not_found::{Trouble, describe_rule};
use crate::pages::studio::{StudioTab, StudioTabs};

/// What a failed change means to the person.
fn failure(error: &ApiError) -> String {
    match error {
        ApiError::TooLarge { .. } => tr("The file is too large.").to_owned(),
        ApiError::Invalid { field, .. } if field == "image" => {
            tr("This picture can't be read. Choose a JPEG, PNG or WebP file.").to_owned()
        }
        ApiError::Invalid { rule, .. } => describe_rule(rule),
        ApiError::Forbidden => tr("You can't see this page.").to_owned(),
        _ => tr("Something went wrong. Try again later.").to_owned(),
    }
}

/// The Customization tab.
#[component]
pub fn Customize() -> impl IntoView {
    let me = SignedIn::resource();
    view! {
        <Title text=tr("Customization")/>
        <main class="studio">
            <StudioTabs current=StudioTab::Customization/>
            <Suspense fallback=|| ()>
                {
                    // the forms hang on the handle alone, so that refreshing the masthead after a
                    // change leaves them, and what they say, as they are
                    let handle = Memo::new(move |_| match me {
                        Some(me) => me.get().map(|me| me.map(|me| me.channel.handle)),
                        None => Some(None),
                    });
                    move || match handle.get() {
                        Some(Some(handle)) => view! { <Loaded handle/> }.into_any(),
                        Some(None) => view! { <Trouble error=ApiError::SignInRequired/> }.into_any(),
                        None => ().into_any(),
                    }
                }
            </Suspense>
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

/// The photo, then the name and description.
#[component]
fn Forms(page: ChannelPage) -> impl IntoView {
    let card = RwSignal::new(page.card.clone());
    let name = RwSignal::new(page.card.name.clone());
    let description = RwSignal::new(page.description.clone());
    let notice = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    // what the masthead shows changes with the channel; the context is read here, where the
    // component's owner is, since the answers come back outside it
    let me = SignedIn::resource();
    let shown = move |profile: Profile| {
        card.set(profile.card);
        if let Some(me) = me {
            me.refetch();
        }
    };
    let save = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        let change = Customization {
            name: Some(name.get_untracked()),
            description: Some(description.get_untracked()),
        };
        busy.set(true);
        leptos::task::spawn_local(async move {
            let answer = call::<profile::Update>(change).await;
            busy.set(false);
            match answer {
                Ok(profile) => {
                    name.set(profile.card.name.clone());
                    description.set(profile.description.clone());
                    shown(profile);
                    notice.set(Some(tr("Saved.").to_owned()));
                }
                Err(error) => notice.set(Some(failure(&error))),
            }
        });
    };
    let choose = move |event: leptos::ev::Event| {
        #[cfg(feature = "hydrate")]
        if let Some(file) = crate::transfer::chosen_file(&event) {
            busy.set(true);
            leptos::task::spawn_local(async move {
                let sent = match crate::transfer::base64_of(&file, profile::PHOTO_BYTES).await {
                    Ok(image) => call::<profile::SetPhoto>(profile::NewPhoto { image }).await,
                    Err(error) => Err(error),
                };
                busy.set(false);
                match sent {
                    Ok(profile) => {
                        shown(profile);
                        notice.set(Some(tr("Saved.").to_owned()));
                    }
                    Err(error) => notice.set(Some(failure(&error))),
                }
            });
        }
        #[cfg(not(feature = "hydrate"))]
        let _ = event;
    };
    let remove = move |_| {
        busy.set(true);
        leptos::task::spawn_local(async move {
            let answer = call::<profile::RemovePhoto>(Nothing {}).await;
            busy.set(false);
            match answer {
                Ok(profile) => shown(profile),
                Err(error) => notice.set(Some(failure(&error))),
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
            <div class="customize-photo">
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
                <input class="field" type="text" required prop:value=name
                    on:input=move |event| name.set(browser::clipped(&event, NAME_LIMIT))/>
            </label>
            <p class="settings-hint"><bdi>{routes::channel(&page.card.handle)}</bdi></p>
            <label class="labelled">
                <span>{tr("Description")}</span>
                <textarea class="field" rows="5" prop:value=description
                    on:input=move |event| description.set(browser::clipped(&event, DESCRIPTION_LIMIT))></textarea>
            </label>
            <div class="editor-actions">
                {move || notice.get().map(|text| view! { <p class="settings-hint" aria-live="polite">{text}</p> })}
                <button class="pill accent" type="submit" disabled=move || busy.get()>{tr("Save")}</button>
            </div>
        </form>
    }
}
