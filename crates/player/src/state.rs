//! The state of one player, and every action on it: the buttons, the keyboard, taps and the
//! seek bar all act through [`Player`], which drives the `<video>` element and the engine.
//!
//! The state follows the element: actions ask the element, and its events write the signals the
//! controls show, so that what is shown is what the element does. On the server, which only
//! renders the markup, actions do nothing.

// Actions come only from the browser; the server renders the markup and never acts.
#![cfg_attr(not(feature = "hydrate"), allow(dead_code))]

use leptos::html::{Div, Video};
use leptos::prelude::*;

#[cfg(feature = "hydrate")]
use crate::engine::Engine;
use crate::keys::Action;

/// The speeds offered, in hundredths of normal.
pub const SPEEDS: [u32; 8] = [25, 50, 75, 100, 125, 150, 175, 200];
/// Normal speed, in hundredths.
pub const NORMAL: u32 = 100;
/// Milliseconds the controls stay after the pointer last moved over playing video.
#[cfg(feature = "hydrate")]
const LINGER: u64 = 3000;

/// The speed `steps` choices away from `speed` among [`SPEEDS`], kept within them.
pub fn step_speed(speed: u32, steps: i32) -> u32 {
    let at = SPEEDS
        .iter()
        .position(|s| *s >= speed)
        .unwrap_or(SPEEDS.len() - 1);
    let last = SPEEDS.len() - 1;
    let to = if steps < 0 {
        at.saturating_sub(steps.unsigned_abs() as usize)
    } else {
        at.saturating_add(steps.unsigned_abs() as usize).min(last)
    };
    SPEEDS.get(to).copied().unwrap_or(NORMAL)
}

/// Which rendition the viewer asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    /// Whichever the bandwidth and the player's size allow.
    Auto,
    /// The rendition this many pixels tall, or the tallest below it.
    Height(u32),
}

/// The panel of the settings menu that is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Menu {
    /// The list of settings.
    Main,
    /// The speeds.
    Speed,
    /// The renditions.
    Quality,
}

/// What the player flashes over the picture after an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    /// It started playing.
    Play,
    /// It paused.
    Pause,
    /// It jumped back this many seconds.
    Back(u32),
    /// It jumped ahead this many seconds.
    Ahead(u32),
    /// The volume is now this many points out of 100; 0 when muted.
    Volume(u32),
    /// The speed is now this many hundredths.
    Speed(u32),
}

/// One player's state; cheap to copy into every handler.
#[derive(Clone, Copy)]
pub struct Player {
    /// The element that plays.
    pub video: NodeRef<Video>,
    /// The element that holds the video and its controls, which fills the screen.
    pub root: NodeRef<Div>,
    /// Whether it is playing.
    pub playing: RwSignal<bool>,
    /// Whether it has played at all.
    pub started: RwSignal<bool>,
    /// Whether it reached the end.
    pub ended: RwSignal<bool>,
    /// The playhead, in seconds.
    pub time: RwSignal<f64>,
    /// The length, in seconds.
    pub duration: RwSignal<f64>,
    /// The end of the media buffered from the playhead on, in seconds.
    pub buffered: RwSignal<f64>,
    /// The volume, in points out of 100.
    pub volume: RwSignal<u32>,
    /// Whether it is muted.
    pub muted: RwSignal<bool>,
    /// The speed, in hundredths.
    pub speed: RwSignal<u32>,
    /// The quality asked for.
    pub quality: RwSignal<Quality>,
    /// The heights of the renditions the browser plays.
    pub renditions: RwSignal<Vec<u32>>,
    /// The height of the rendition playing.
    pub height: RwSignal<Option<u32>>,
    /// Whether the page shows the player in theatre mode; `None` on a page that has none.
    pub theatre: Option<RwSignal<bool>>,
    /// Whether the player fills the screen.
    pub full_screen: RwSignal<bool>,
    /// Whether the controls show.
    pub active: RwSignal<bool>,
    /// The open panel of the settings menu.
    pub menu: RwSignal<Option<Menu>>,
    /// The last notice, numbered so that the same one twice flashes twice.
    pub notice: RwSignal<Option<(u32, Notice)>>,
    /// Whether it waits for data.
    pub waiting: RwSignal<bool>,
    /// Whether nothing here can play it.
    pub unplayable: RwSignal<bool>,
    /// Counts pointer moves, so that only the last one hides the controls.
    moves: StoredValue<u32>,
    #[cfg(feature = "hydrate")]
    engine: StoredValue<Option<Engine>, LocalStorage>,
}

impl Player {
    /// A player of a video lasting `duration` seconds, shown in theatre mode as `theatre` says.
    pub fn new(duration: f64, theatre: Option<RwSignal<bool>>) -> Player {
        Player {
            video: NodeRef::new(),
            root: NodeRef::new(),
            playing: RwSignal::new(false),
            started: RwSignal::new(false),
            ended: RwSignal::new(false),
            time: RwSignal::new(0.0),
            duration: RwSignal::new(duration),
            buffered: RwSignal::new(0.0),
            volume: RwSignal::new(100),
            muted: RwSignal::new(false),
            speed: RwSignal::new(NORMAL),
            quality: RwSignal::new(Quality::Auto),
            renditions: RwSignal::new(Vec::new()),
            height: RwSignal::new(None),
            theatre,
            full_screen: RwSignal::new(false),
            active: RwSignal::new(true),
            menu: RwSignal::new(None),
            notice: RwSignal::new(None),
            waiting: RwSignal::new(false),
            unplayable: RwSignal::new(false),
            moves: StoredValue::new(0),
            #[cfg(feature = "hydrate")]
            engine: StoredValue::new_local(None),
        }
    }

    /// Flash `notice` over the picture.
    pub fn flash(&self, notice: Notice) {
        self.notice
            .update(|last| *last = Some((last.map_or(0, |(n, _)| n.wrapping_add(1)), notice)));
    }

    /// Show the controls, and hide them again after a while if the video plays on.
    pub fn poke(&self) {
        self.active.set(true);
        let moves = self.moves.get_value().wrapping_add(1);
        self.moves.set_value(moves);
        #[cfg(feature = "hydrate")]
        {
            let player = *self;
            set_timeout(
                move || {
                    let idle = player.moves.try_get_value() == Some(moves);
                    if idle
                        && player.playing.get_untracked()
                        && player.menu.get_untracked().is_none()
                    {
                        player.active.set(false);
                    }
                },
                std::time::Duration::from_millis(LINGER),
            );
        }
    }

    /// Do what a key asked.
    pub fn act(&self, action: Action) {
        match action {
            Action::TogglePlay => self.toggle_play(),
            Action::Jump(seconds) => self.jump(f64::from(seconds)),
            Action::JumpToTenth(tenth) => self.seek_fraction(f64::from(tenth) / 10.0),
            Action::JumpToStart => self.seek(0.0),
            Action::JumpToEnd => self.seek(self.duration.get_untracked()),
            Action::Volume(points) => {
                let volume = i64::from(self.shown_volume()) + i64::from(points);
                self.set_volume(u32::try_from(volume.clamp(0, 100)).unwrap_or(0));
                self.flash(Notice::Volume(self.volume.get_untracked()));
            }
            Action::ToggleMute => {
                self.toggle_mute();
                let shown = if self.muted.get_untracked() {
                    0
                } else {
                    self.volume.get_untracked()
                };
                self.flash(Notice::Volume(shown));
            }
            Action::ToggleFullScreen => self.toggle_full_screen(),
            Action::ToggleTheatre => {
                if let Some(theatre) = self.theatre {
                    theatre.update(|on| *on = !*on);
                }
            }
            Action::Speed(steps) => {
                let speed = step_speed(self.speed.get_untracked(), steps);
                self.set_speed(speed);
                self.flash(Notice::Speed(speed));
            }
            Action::Frame(frames) => {
                if !self.playing.get_untracked() {
                    self.seek(self.time.get_untracked() + f64::from(frames) / 30.0);
                }
            }
        }
        self.poke();
    }

    /// The volume the controls show: 0 while muted.
    pub fn shown_volume(&self) -> u32 {
        if self.muted.get_untracked() {
            0
        } else {
            self.volume.get_untracked()
        }
    }

    /// Jump `seconds` ahead (or back, when negative), and say so.
    pub fn jump(&self, seconds: f64) {
        self.seek(self.time.get_untracked() + seconds);
        let whole = seconds.abs().round() as u32;
        self.flash(if seconds < 0.0 {
            Notice::Back(whole)
        } else {
            Notice::Ahead(whole)
        });
    }

    /// Move the playhead to `fraction` of the video.
    pub fn seek_fraction(&self, fraction: f64) {
        self.seek(fraction.clamp(0.0, 1.0) * self.duration.get_untracked());
    }

    /// Ask the engine for `quality`.
    pub fn set_quality(&self, quality: Quality) {
        self.quality.set(quality);
        #[cfg(feature = "hydrate")]
        self.engine.with_value(|engine| {
            if let Some(engine) = engine {
                engine.set_quality(quality);
            }
        });
    }
}

#[cfg(feature = "hydrate")]
mod browser {
    //! The actions, on the browser's element.

    use leptos::prelude::*;
    use web_sys::HtmlVideoElement;

    use super::{Notice, Player};
    use crate::engine::{Engine, Report};
    use crate::web;

    const VOLUME_KEY: &str = "youcraft.volume";

    impl Player {
        fn element(&self) -> Option<HtmlVideoElement> {
            self.video.get_untracked()
        }

        /// Start streaming `master` into the element, and play when the browser allows.
        pub fn start(&self, master: String) {
            let Some(video) = self.element() else {
                return;
            };
            self.restore_volume(&video);
            let player = *self;
            let engine = Engine::start(video.clone(), master, move |report| match report {
                Report::Renditions(heights) => player.renditions.set(heights),
                Report::Height(height) => player.height.set(Some(height)),
                Report::Unplayable => player.unplayable.set(true),
            });
            self.engine.set_value(Some(engine));
            self.play(&video);
        }

        /// Stop streaming, as the page leaves.
        pub fn stop(&self) {
            self.engine.try_with_value(|engine| {
                if let Some(engine) = engine {
                    engine.stop();
                }
            });
        }

        fn play(&self, video: &HtmlVideoElement) {
            if let Ok(promise) = video.play() {
                // A browser that wants a tap first refuses; the play button stays.
                wasm_bindgen_futures::spawn_local(async move {
                    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                });
            }
        }

        /// Play if paused, pause if playing.
        pub fn toggle_play(&self) {
            let Some(video) = self.element() else {
                return;
            };
            if video.paused() || video.ended() {
                if video.ended() {
                    video.set_current_time(0.0);
                }
                self.play(&video);
                self.flash(Notice::Play);
            } else {
                let _ = video.pause();
                self.flash(Notice::Pause);
            }
        }

        /// Move the playhead to `seconds`, kept within the video.
        pub fn seek(&self, seconds: f64) {
            if let Some(video) = self.element() {
                let end = self.duration.get_untracked().max(0.0);
                let to = seconds.clamp(0.0, end);
                video.set_current_time(to);
                self.time.set(to);
            }
        }

        /// Set the volume to `points` out of 100, unmuting unless it is 0.
        pub fn set_volume(&self, points: u32) {
            if let Some(video) = self.element() {
                video.set_volume(f64::from(points.min(100)) / 100.0);
                video.set_muted(points == 0);
                self.save_volume(points);
            }
        }

        /// Mute or unmute.
        pub fn toggle_mute(&self) {
            if let Some(video) = self.element() {
                let muted = !video.muted();
                video.set_muted(muted);
                if !muted && video.volume() == 0.0 {
                    video.set_volume(1.0);
                }
            }
        }

        /// Set the speed to `hundredths` of normal.
        pub fn set_speed(&self, hundredths: u32) {
            if let Some(video) = self.element() {
                video.set_playback_rate(f64::from(hundredths) / 100.0);
            }
        }

        /// Fill the screen with the player, or stop filling it.
        pub fn toggle_full_screen(&self) {
            let document = web::window().ok().and_then(|window| window.document());
            if let Some(document) = &document
                && document.fullscreen_element().is_some()
            {
                document.exit_fullscreen();
                return;
            }
            if web::full_screen_enabled() {
                if let Some(root) = self.root.get_untracked() {
                    let _ = root.request_fullscreen();
                }
            } else if let Some(video) = self.element() {
                web::video_full_screen(&video);
            }
        }

        /// Read what the element now does into the signals.
        pub fn sync(&self) {
            let Some(video) = self.element() else {
                return;
            };
            let time = video.current_time();
            self.time.set(time);
            let duration = video.duration();
            if duration.is_finite() && duration > 0.0 {
                self.duration.set(duration);
            }
            let ranges = video.buffered();
            let end = (0..ranges.length())
                .filter_map(|i| Some((ranges.start(i).ok()?, ranges.end(i).ok()?)))
                .find(|(start, end)| *start - 0.1 <= time && time <= *end + 0.1)
                .map_or(time, |(_, end)| end);
            self.buffered.set(end);
            self.playing.set(!video.paused() && !video.ended());
            self.ended.set(video.ended());
            self.muted.set(video.muted());
            self.volume.set((video.volume() * 100.0).round() as u32);
            self.speed
                .set((video.playback_rate() * 100.0).round() as u32);
        }

        fn storage() -> Option<web_sys::Storage> {
            web::window().ok()?.local_storage().ok()?
        }

        fn save_volume(&self, points: u32) {
            if let Some(storage) = Self::storage() {
                let _ = storage.set_item(VOLUME_KEY, &points.to_string());
            }
        }

        fn restore_volume(&self, video: &HtmlVideoElement) {
            let saved = Self::storage()
                .and_then(|storage| storage.get_item(VOLUME_KEY).ok().flatten())
                .and_then(|text| text.parse::<u32>().ok());
            if let Some(points) = saved {
                video.set_volume(f64::from(points.min(100)) / 100.0);
                video.set_muted(points == 0);
            }
        }
    }
}

#[cfg(not(feature = "hydrate"))]
impl Player {
    /// Play if paused, pause if playing; nothing on the server.
    pub fn toggle_play(&self) {}
    /// Move the playhead; nothing on the server.
    pub fn seek(&self, _seconds: f64) {}
    /// Set the volume; nothing on the server.
    pub fn set_volume(&self, _points: u32) {}
    /// Mute or unmute; nothing on the server.
    pub fn toggle_mute(&self) {}
    /// Set the speed; nothing on the server.
    pub fn set_speed(&self, _hundredths: u32) {}
    /// Fill the screen; nothing on the server.
    pub fn toggle_full_screen(&self) {}
    /// Read the element; nothing on the server.
    pub fn sync(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speeds_step_within_the_choices() {
        assert_eq!(step_speed(100, 1), 125);
        assert_eq!(step_speed(100, -1), 75);
        assert_eq!(step_speed(200, 1), 200);
        assert_eq!(step_speed(25, -3), 25);
        assert_eq!(step_speed(110, 0), 125);
        assert_eq!(step_speed(900, -1), 175);
    }
}
