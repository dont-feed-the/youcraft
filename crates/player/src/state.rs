//! The state of one player, and every action on it: the buttons, the keyboard, taps and the
//! seek bar all act through [`Player`], which drives the `<video>` element and the engine.
//!
//! The state follows the element: actions ask the element, and its events write the signals the
//! controls show, each only when its value changes, so that what is shown is what the element
//! does and nothing is drawn again for nothing. Times are nanoseconds, as [`crate::time`] counts
//! them. On the server, which only renders the markup, actions do nothing.

// Actions come only from the browser: the server renders the markup and never acts, so what
// only actions use is dead there, and checked where it lives, in the `hydrate` build.
#![cfg_attr(not(feature = "hydrate"), allow(dead_code))]

use leptos::html::{Div, Video};
use leptos::prelude::*;
use youcraft_api::time::MediaDuration;

#[cfg(feature = "hydrate")]
use crate::engine::Engine;
#[cfg(feature = "hydrate")]
use crate::keys::Action;
use crate::time::NANOS;

/// The speeds offered, in hundredths of normal.
pub const SPEEDS: [u32; 8] = [25, 50, 75, 100, 125, 150, 175, 200];
/// Normal speed, in hundredths.
pub const NORMAL: u32 = 100;
/// Milliseconds the controls stay after the pointer last moved over playing video.
#[cfg(feature = "hydrate")]
const LINGER: u64 = 3000;

/// The speed `steps` choices away from `speed` among [`SPEEDS`]: the `steps`-th choice above
/// it, or below it when `steps` is negative, kept within the choices; `speed` itself for none.
pub fn step_speed(speed: u32, steps: i32) -> u32 {
    let last = SPEEDS.len() - 1;
    let count = steps.unsigned_abs() as usize;
    let to = match steps.signum() {
        0 => return speed,
        1 => SPEEDS
            .partition_point(|choice| *choice <= speed)
            .saturating_add(count - 1)
            .min(last),
        _ => SPEEDS
            .partition_point(|choice| *choice < speed)
            .saturating_sub(count),
    };
    SPEEDS.get(to).copied().unwrap_or(NORMAL)
}

/// Which rendition the viewer asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    /// Whichever the bandwidth and the player's size allow.
    Auto,
    /// The rendition of this many lines, as "720p" counts them, or the largest below it.
    Lines(u32),
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

/// Write `value` into `signal` only when it differs, so that nothing is drawn again for nothing.
fn put<T: PartialEq + Send + Sync + 'static>(signal: RwSignal<T>, value: T) {
    signal.maybe_update(|old| {
        let changed = *old != value;
        if changed {
            *old = value;
        }
        changed
    });
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
    /// The playhead, in nanoseconds.
    pub time: RwSignal<u64>,
    /// The length, in nanoseconds.
    pub duration: RwSignal<u64>,
    /// The end of the media buffered from the playhead on, in nanoseconds.
    pub buffered: RwSignal<u64>,
    /// How long a frame lasts, once known.
    pub frame: RwSignal<Option<MediaDuration>>,
    /// The volume, in points out of 100.
    pub volume: RwSignal<u32>,
    /// Whether it is muted.
    pub muted: RwSignal<bool>,
    /// The speed, in hundredths.
    pub speed: RwSignal<u32>,
    /// The quality asked for.
    pub quality: RwSignal<Quality>,
    /// The lines of the renditions the browser plays, as "720p" counts them, lowest first.
    pub renditions: RwSignal<Vec<u32>>,
    /// The lines of the picture playing, once known.
    pub lines: RwSignal<Option<u32>>,
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
    /// Whether its playlists could not be had.
    pub failed: RwSignal<bool>,
    /// Counts pointer moves, so that only the last one hides the controls.
    moves: StoredValue<u32>,
    #[cfg(feature = "hydrate")]
    engine: StoredValue<Option<Engine>, LocalStorage>,
}

impl Player {
    /// A player of a video lasting `duration` nanoseconds, shown in theatre mode as `theatre`
    /// says.
    pub fn new(duration: u64, theatre: Option<RwSignal<bool>>) -> Player {
        Player {
            video: NodeRef::new(),
            root: NodeRef::new(),
            playing: RwSignal::new(false),
            started: RwSignal::new(false),
            ended: RwSignal::new(false),
            time: RwSignal::new(0),
            duration: RwSignal::new(duration),
            buffered: RwSignal::new(0),
            frame: RwSignal::new(None),
            volume: RwSignal::new(100),
            muted: RwSignal::new(false),
            speed: RwSignal::new(NORMAL),
            quality: RwSignal::new(Quality::Auto),
            renditions: RwSignal::new(Vec::new()),
            lines: RwSignal::new(None),
            theatre,
            full_screen: RwSignal::new(false),
            active: RwSignal::new(true),
            menu: RwSignal::new(None),
            notice: RwSignal::new(None),
            waiting: RwSignal::new(false),
            unplayable: RwSignal::new(false),
            failed: RwSignal::new(false),
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
    #[cfg(feature = "hydrate")]
    pub fn act(&self, action: Action) {
        match action {
            Action::TogglePlay => self.toggle_play(),
            Action::Jump(seconds) => self.jump(seconds),
            Action::JumpToTenth(tenth) => {
                let to = u128::from(self.duration.get_untracked()) * u128::from(tenth) / 10;
                self.seek(u64::try_from(to).unwrap_or(u64::MAX));
            }
            Action::JumpToStart => self.seek(0),
            Action::JumpToEnd => self.seek(self.duration.get_untracked()),
            Action::Volume(points) => {
                let volume = i64::from(self.shown_volume()) + i64::from(points);
                self.set_volume(u32::try_from(volume.clamp(0, 100)).unwrap_or(0));
                self.flash(Notice::Volume(self.volume.get_untracked()));
            }
            Action::ToggleMute => {
                self.toggle_mute();
                self.flash(Notice::Volume(self.shown_volume()));
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
                let to = self.frame.get_untracked().and_then(|frame| {
                    crate::time::step_frames(self.time.get_untracked(), frames, frame)
                });
                if let Some(to) = to
                    && !self.playing.get_untracked()
                {
                    self.seek(to);
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
    pub fn jump(&self, seconds: i32) {
        let (now, by) = (
            self.time.get_untracked(),
            u64::from(seconds.unsigned_abs()) * NANOS,
        );
        if seconds < 0 {
            self.seek(now.saturating_sub(by));
            self.flash(Notice::Back(seconds.unsigned_abs()));
        } else {
            self.seek(now.saturating_add(by));
            self.flash(Notice::Ahead(seconds.unsigned_abs()));
        }
    }

    /// Move the playhead to `fraction` of the video, as a pointer along the seek bar asks.
    pub fn seek_fraction(&self, fraction: f64) {
        let duration = self.duration.get_untracked();
        self.seek((fraction.clamp(0.0, 1.0) * duration as f64) as u64);
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

    use super::{Notice, Player, put};
    use crate::engine::{Engine, Report};
    use crate::playlist::picture_lines;
    use crate::schedule::range_at;
    use crate::time;
    use crate::web;

    /// Where the volume is kept, in points out of 100.
    const VOLUME_KEY: &str = "youcraft.volume";
    /// Where whether it is muted is kept: `1` or `0`.
    const MUTED_KEY: &str = "youcraft.muted";

    impl Player {
        fn element(&self) -> Option<HtmlVideoElement> {
            self.video.get_untracked()
        }

        /// Start streaming `master` into the element, and play when the browser allows.
        pub fn start(&self, master: String) {
            let Some(video) = self.element() else {
                return;
            };
            Self::restore_volume(&video);
            let player = *self;
            let engine = Engine::start(video.clone(), master, move |report| match report {
                Report::Renditions(lines) => put(player.renditions, lines),
                Report::Frame(frame) => put(player.frame, Some(frame)),
                Report::Unplayable => player.unplayable.set(true),
                Report::Failed => player.failed.set(true),
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

        /// Move the playhead to `nanos`, kept within the video.
        pub fn seek(&self, nanos: u64) {
            if let Some(video) = self.element() {
                let to = nanos.min(self.duration.get_untracked());
                video.set_current_time(time::seconds(to));
                put(self.time, to);
            }
        }

        /// Set the volume to `points` out of 100, unmuting unless it is 0.
        pub fn set_volume(&self, points: u32) {
            if let Some(video) = self.element() {
                video.set_volume(f64::from(points.min(100)) / 100.0);
                video.set_muted(points == 0);
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

        /// Read what the element now does into the signals, and keep the volume it was left at.
        pub fn sync(&self) {
            let Some(video) = self.element() else {
                return;
            };
            let now = time::from_seconds(video.current_time());
            put(self.time, now);
            let duration = video.duration();
            if duration.is_finite() && duration > 0.0 {
                put(self.duration, time::from_seconds(duration));
            }
            let buffered =
                range_at(&web::ranges(&video.buffered()), now).map_or(now, |(_, end)| end);
            put(self.buffered, buffered);
            put(self.playing, !video.paused() && !video.ended());
            put(self.ended, video.ended());
            let (muted, volume) = (video.muted(), (video.volume() * 100.0).round() as u32);
            if (muted, volume) != (self.muted.get_untracked(), self.volume.get_untracked()) {
                put(self.muted, muted);
                put(self.volume, volume);
                Self::save_volume(volume, muted);
            }
            put(self.speed, (video.playback_rate() * 100.0).round() as u32);
        }

        /// Read the size of the picture playing into the lines it is known by.
        pub fn sync_picture(&self) {
            if let Some(video) = self.element() {
                let (width, height) = (video.video_width(), video.video_height());
                put(
                    self.lines,
                    (height > 0).then(|| picture_lines(width, height)),
                );
            }
        }

        fn storage() -> Option<web_sys::Storage> {
            web::window().ok()?.local_storage().ok()?
        }

        fn save_volume(points: u32, muted: bool) {
            if let Some(storage) = Self::storage() {
                let _ = storage.set_item(VOLUME_KEY, &points.to_string());
                let _ = storage.set_item(MUTED_KEY, if muted { "1" } else { "0" });
            }
        }

        fn restore_volume(video: &HtmlVideoElement) {
            let Some(storage) = Self::storage() else {
                return;
            };
            let read = |key| storage.get_item(key).ok().flatten();
            let points = read(VOLUME_KEY).and_then(|text| text.parse::<u32>().ok());
            if let Some(points) = points {
                video.set_volume(f64::from(points.min(100)) / 100.0);
            }
            // Before mute was kept, a volume of 0 meant muted.
            let muted = read(MUTED_KEY).map_or(points == Some(0), |text| text == "1");
            video.set_muted(muted);
        }
    }
}

#[cfg(not(feature = "hydrate"))]
impl Player {
    /// Play if paused, pause if playing; nothing on the server.
    pub fn toggle_play(&self) {}
    /// Move the playhead; nothing on the server.
    pub fn seek(&self, _nanos: u64) {}
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
    /// Read the picture's size; nothing on the server.
    pub fn sync_picture(&self) {}
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    #[test]
    fn signals_are_written_only_when_their_value_changes() {
        // Every timeupdate writes about ten signals; what reads one that did not change, such
        // as the open settings menu reading the speed, must not run again.
        Owner::new().with(|| {
            let speed = RwSignal::new(NORMAL);
            let runs = Arc::new(AtomicU32::new(0));
            let counted = Arc::clone(&runs);
            let label = Memo::new(move |_| {
                counted.fetch_add(1, Ordering::SeqCst);
                speed.get()
            });
            assert_eq!(label.get(), NORMAL);
            put(speed, NORMAL);
            assert_eq!(label.get(), NORMAL);
            assert_eq!(runs.load(Ordering::SeqCst), 1);
            put(speed, 125);
            assert_eq!(label.get(), 125);
            assert_eq!(runs.load(Ordering::SeqCst), 2);
        });
    }

    #[test]
    fn speeds_step_to_the_next_choices() {
        assert_eq!(step_speed(100, 1), 125);
        assert_eq!(step_speed(100, -1), 75);
        assert_eq!(step_speed(100, 2), 150);
        assert_eq!(step_speed(200, 1), 200);
        assert_eq!(step_speed(25, -3), 25);
        // A speed between the choices steps to the nearest one on that side; no step keeps it.
        assert_eq!(step_speed(110, 1), 125);
        assert_eq!(step_speed(110, -1), 100);
        assert_eq!(step_speed(110, 0), 110);
        assert_eq!(step_speed(900, -1), 200);
        assert_eq!(step_speed(10, 1), 25);
        assert_eq!(step_speed(100, i32::MIN), 25);
        assert_eq!(step_speed(100, i32::MAX), 200);
    }
}
