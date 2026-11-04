//! The account end of the masthead: the avatar and its menu when signed in; the settings menu
//! and the sign-in button when not. The menu opens on panels as YouTube's does: the account and
//! its places first (its channel, the studio, the settings, the administration for
//! administrators, signing out), then appearance and language, each with the current choice
//! ticked, then the project's places beyond the site.

use leptos::prelude::*;
use leptos_router::hooks::use_location;
use youcraft_api::commands::account::{Me, Theme};
use youcraft_api::{Grant, routes};

use crate::account::{Shown, SignedIn, choose_language, choose_theme, sign_out};
use crate::components::avatar::{Face, Size};
use crate::components::project;
use crate::icons::{Glyph, Icon};
use crate::lang::{Language, fill, offered, tr};

/// Which panel of the menu shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Panel {
    Main,
    Appearance,
    Language,
}

/// The name of `theme` as the menu's entry shows its value.
fn theme_name(theme: Theme) -> &'static str {
    match theme {
        Theme::Device => tr("Device theme"),
        Theme::Dark => tr("Dark"),
        Theme::Light => tr("Light"),
    }
}

/// The account end of the masthead.
#[component]
pub fn AccountArea() -> impl IntoView {
    let open = RwSignal::new(None::<Panel>);
    let location = use_location();
    let here = move || format!("{}{}", location.pathname.get(), location.search.get());
    let me = SignedIn::resource();
    // The masthead outlives every page, so the menu would stay open on the next one.
    Effect::watch(here, move |_, _, _| open.set(None), false);
    let toggle = move |_| {
        open.update(|panel| {
            *panel = if panel.is_some() {
                None
            } else {
                Some(Panel::Main)
            }
        })
    };
    view! {
        <Suspense fallback=|| view! { <span class="avatar"></span> }>
            {move || Suspend::new(async move {
                let me = match me {
                    Some(me) => me.await,
                    None => None,
                };
                let expanded = move || if open.get().is_some() { "true" } else { "false" };
                match me {
                    Some(me) => view! {
                        <button class="icon-button" type="button" aria-label=tr("Account menu")
                            aria-haspopup="menu" aria-expanded=expanded on:click=toggle>
                            <span class="avatar"><Face channel=me.channel.clone() size=Size::Regular/></span>
                        </button>
                        <Menu me=Some(me) open/>
                    }.into_any(),
                    None => view! {
                        <button class="icon-button" type="button" aria-label=tr("Settings")
                            aria-haspopup="menu" aria-expanded=expanded on:click=toggle>
                            <Glyph icon=Icon::More/>
                        </button>
                        <a class="sign-in" rel="external" href=move || routes::sign_in(&here())>
                            <Glyph icon=Icon::Account/>
                            <span>{tr("Sign in")}</span>
                        </a>
                        <Menu me=None open/>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}

/// The menu, while `open` names a panel.
#[component]
fn Menu(me: Option<Me>, open: RwSignal<Option<Panel>>) -> impl IntoView {
    let me = StoredValue::new(me);
    let close = move |_| open.set(None);
    let escape = move |event: leptos::ev::KeyboardEvent| {
        if event.key() == "Escape" {
            open.set(None);
        }
    };
    view! {
        <Show when=move || open.get().is_some()>
            <div class="menu-backdrop" on:click=close></div>
            <div class="menu" role="menu" on:keydown=escape>
                {move || match open.get() {
                    Some(Panel::Appearance) => view! { <AppearancePanel me open/> }.into_any(),
                    Some(Panel::Language) => view! { <LanguagePanel me open/> }.into_any(),
                    _ => view! { <MainPanel me open/> }.into_any(),
                }}
            </div>
        </Show>
    }
}

/// The first panel: the account, its places, the entries that open the other panels, and the
/// project's places.
#[component]
fn MainPanel(me: StoredValue<Option<Me>>, open: RwSignal<Option<Panel>>) -> impl IntoView {
    let shown = use_context::<Shown>().map(|Shown(shown)| shown);
    let theme = move || shown.map_or(Theme::Device, |shown| shown.get());
    let code = youcraft_i18n::code(Language::current());
    let language = offered()
        .into_iter()
        .find(|(offered, _)| offered == code)
        .map_or_else(|| code.to_owned(), |(_, name)| name);
    let account = me.get_value().map(|me| {
        let channel = routes::channel(&me.channel.handle);
        view! {
            <div class="menu-account">
                <span class="avatar"><Face channel=me.channel.clone() size=Size::Regular/></span>
                <div class="menu-account-names">
                    <span class="menu-account-name">{me.channel.name.clone()}</span>
                    <bdi class="menu-account-handle">{format!("@{}", me.channel.handle)}</bdi>
                </div>
            </div>
            <hr class="menu-rule"/>
            <MenuLink href=channel icon=Icon::Account label=tr("Your channel") open/>
            <MenuLink href=routes::STUDIO icon=Icon::Studio label=tr("YouCraft Studio") open/>
            <MenuLink href=routes::ACCOUNT icon=Icon::Settings label=tr("Settings") open/>
            {(me.role >= Grant::Admin).then(|| view! {
                <MenuLink href=routes::ADMIN icon=Icon::Shield label=tr("Administration") open/>
            })}
            <button class="menu-entry" role="menuitem" type="button" on:click=move |_| sign_out()>
                <Glyph icon=Icon::SignOut/><span>{tr("Sign out")}</span>
            </button>
            <hr class="menu-rule"/>
        }
    });
    view! {
        <div class="menu-heading menu-close">
            <button class="icon-button" type="button" aria-label=tr("Back") on:click=move |_| open.set(None)>
                <Glyph icon=Icon::Back/>
            </button>
        </div>
        {account}
        <button class="menu-entry" role="menuitem" type="button" aria-haspopup="menu"
            on:click=move |_| open.set(Some(Panel::Appearance))>
            <Glyph icon=Icon::Appearance/>
            <span>{move || fill(tr("Appearance: {theme}"), &[("theme", theme_name(theme()))])}</span>
            <span class="menu-more"><Glyph icon=Icon::Forward/></span>
        </button>
        <button class="menu-entry" role="menuitem" type="button" aria-haspopup="menu"
            on:click=move |_| open.set(Some(Panel::Language))>
            <Glyph icon=Icon::Translate/>
            <span>{fill(tr("Language: {language}"), &[("language", &language)])}</span>
            <span class="menu-more"><Glyph icon=Icon::Forward/></span>
        </button>
        <hr class="menu-rule"/>
        {project::places().map(|place| view! {
            <MenuLink href=place.href icon=place.icon label=place.label external=true open/>
        }).into_iter().collect_view()}
    }
}

/// A link of the menu. Following it closes the menu, though it leads to the page already shown,
/// or off the site, where going back would find the menu open still.
#[component]
fn MenuLink(
    #[prop(into)] href: String,
    icon: Icon,
    label: &'static str,
    #[prop(optional)] external: bool,
    open: RwSignal<Option<Panel>>,
) -> impl IntoView {
    view! {
        <a class="menu-entry" role="menuitem" href=href rel=external.then_some("external")
            on:click=move |_| open.set(None)>
            <Glyph icon=icon/><span>{label}</span>
        </a>
    }
}

/// The heading of a panel inside the menu, with the way back to the first.
#[component]
fn PanelHeading(title: &'static str, open: RwSignal<Option<Panel>>) -> impl IntoView {
    view! {
        <div class="menu-heading">
            <button class="icon-button" type="button" aria-label=tr("Back")
                on:click=move |_| open.set(Some(Panel::Main))>
                <Glyph icon=Icon::Back/>
            </button>
            <span>{title}</span>
        </div>
        <hr class="menu-rule"/>
    }
}

/// The themes, the current one ticked.
#[component]
fn AppearancePanel(me: StoredValue<Option<Me>>, open: RwSignal<Option<Panel>>) -> impl IntoView {
    let shown = use_context::<Shown>().map(|Shown(shown)| shown);
    let entries = Theme::ALL.map(|theme| {
        let label = match theme {
            Theme::Device => tr("Use device theme"),
            Theme::Dark => tr("Dark theme"),
            Theme::Light => tr("Light theme"),
        };
        let ticked = move || shown.is_some_and(|shown| shown.get() == theme);
        let choose = move |_| {
            me.with_value(|me| choose_theme(theme, me.as_ref()));
            open.set(None);
        };
        view! {
            <button class="menu-entry" role="menuitemradio" type="button"
                aria-checked=move || if ticked() { "true" } else { "false" } on:click=choose>
                <span class="menu-tick" class:ticked=ticked><Glyph icon=Icon::Check/></span>
                <span>{label}</span>
            </button>
        }
    });
    view! {
        <PanelHeading title=tr("Appearance") open/>
        {entries.into_iter().collect_view()}
    }
}

/// The languages on offer, each in its own name, the current one ticked.
#[component]
fn LanguagePanel(me: StoredValue<Option<Me>>, open: RwSignal<Option<Panel>>) -> impl IntoView {
    let current = youcraft_i18n::code(Language::current());
    let entries = offered().into_iter().map(|(code, name)| {
        let ticked = code == current;
        let chosen = code.clone();
        let choose = move |_| {
            me.with_value(|me| choose_language(chosen.clone(), me.as_ref()));
            open.set(None);
        };
        view! {
            <button class="menu-entry" role="menuitemradio" type="button" lang=code
                aria-checked=if ticked { "true" } else { "false" } on:click=choose>
                <span class="menu-tick" class:ticked=ticked><Glyph icon=Icon::Check/></span>
                <span>{name}</span>
            </button>
        }
    });
    view! {
        <PanelHeading title=tr("Language") open/>
        <div class="menu-scroll">{entries.collect_view()}</div>
    }
}
