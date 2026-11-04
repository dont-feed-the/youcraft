//! The settings menu: the speed and the quality, each a list of choices behind one line of the
//! main panel, as on youtube.com. Its text runs in the page's direction.

use leptos::prelude::*;
use youcraft_i18n::page::{Language, tr};

use crate::icons::{Glyph, Icon};
use crate::state::{Menu, NORMAL, Player, Quality, SPEEDS};

/// How a speed of `hundredths` of normal reads: "Normal", or the factor, "1.25".
pub fn speed_label(hundredths: u32) -> String {
    if hundredths == NORMAL {
        return tr("Normal").to_owned();
    }
    let places = match hundredths % 100 {
        0 => 0,
        rest if rest % 10 == 0 => 1,
        _ => 2,
    };
    youcraft_i18n::decimal(Language::current(), f64::from(hundredths) / 100.0, places)
}

/// How a rendition of `lines` lines reads: "720p".
fn lines_label(lines: u32) -> String {
    format!("{lines}p")
}

/// How the quality reads in the main panel: "Auto (360p)" while automatic, with the lines of
/// the picture playing, else "720p".
fn quality_label(player: Player) -> String {
    match player.quality.get() {
        Quality::Lines(lines) => lines_label(lines),
        Quality::Auto => match player.lines.get() {
            Some(lines) => {
                youcraft_i18n::fill(tr("Auto ({quality})"), &[("quality", &lines_label(lines))])
            }
            None => tr("Auto").to_owned(),
        },
    }
}

/// The settings menu of `player`, its text in `page_dir`.
#[component]
pub fn Settings(player: Player, page_dir: &'static str) -> impl IntoView {
    let open = move |menu: Menu| move |_| player.menu.set(Some(menu));
    let back = move |_| player.menu.set(Some(Menu::Main));
    view! {
        <Show when=move || player.menu.get().is_some()>
            // The menu stands over its button, at the end of the bar, which runs left to right;
            // only its panel's text runs as the page's does.
            <div class="player-menu">
                <div class="menu-panel" dir=page_dir role="menu">
                    {move || match player.menu.get() {
                        Some(Menu::Main) | None => view! {
                            <button class="menu-row" type="button" role="menuitem" on:click=open(Menu::Speed)>
                                <span class="menu-label">{tr("Playback speed")}</span>
                                <span class="menu-value">{move || speed_label(player.speed.get())}</span>
                                <Glyph icon=Icon::Next/>
                            </button>
                            <Show when=move || !player.renditions.get().is_empty()>
                                <button class="menu-row" type="button" role="menuitem" on:click=open(Menu::Quality)>
                                    <span class="menu-label">{tr("Quality")}</span>
                                    <span class="menu-value">{move || quality_label(player)}</span>
                                    <Glyph icon=Icon::Next/>
                                </button>
                            </Show>
                        }
                        .into_any(),
                        Some(Menu::Speed) => view! {
                            <button class="menu-row menu-title" type="button" on:click=back>
                                <Glyph icon=Icon::Back/>
                                <span class="menu-label">{tr("Playback speed")}</span>
                            </button>
                            {SPEEDS
                                .into_iter()
                                .map(|speed| {
                                    let chosen = move || player.speed.get() == speed;
                                    view! {
                                        <button
                                            class="menu-row menu-choice"
                                            type="button"
                                            role="menuitemradio"
                                            aria-checked=move || chosen().to_string()
                                            on:click=move |_| {
                                                player.set_speed(speed);
                                                player.menu.set(None);
                                            }
                                        >
                                            <span class="menu-check">{move || chosen().then(|| view! { <Glyph icon=Icon::Check/> })}</span>
                                            <span class="menu-label">{speed_label(speed)}</span>
                                        </button>
                                    }
                                })
                                .collect_view()}
                        }
                        .into_any(),
                        Some(Menu::Quality) => {
                            let mut choices: Vec<Quality> = player
                                .renditions
                                .get_untracked()
                                .into_iter()
                                .rev()
                                .map(Quality::Lines)
                                .collect();
                            choices.push(Quality::Auto);
                            view! {
                                <button class="menu-row menu-title" type="button" on:click=back>
                                    <Glyph icon=Icon::Back/>
                                    <span class="menu-label">{tr("Quality")}</span>
                                </button>
                                {choices
                                    .into_iter()
                                    .map(|quality| {
                                        let chosen = move || player.quality.get() == quality;
                                        let label = match quality {
                                            Quality::Auto => tr("Auto").to_owned(),
                                            Quality::Lines(lines) => lines_label(lines),
                                        };
                                        view! {
                                            <button
                                                class="menu-row menu-choice"
                                                type="button"
                                                role="menuitemradio"
                                                aria-checked=move || chosen().to_string()
                                                on:click=move |_| {
                                                    player.set_quality(quality);
                                                    player.menu.set(None);
                                                }
                                            >
                                                <span class="menu-check">{move || chosen().then(|| view! { <Glyph icon=Icon::Check/> })}</span>
                                                <span class="menu-label">{label}</span>
                                            </button>
                                        }
                                    })
                                    .collect_view()}
                            }
                            .into_any()
                        }
                    }}
                </div>
            </div>
        </Show>
    }
    .into_any()
}
