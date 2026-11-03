//! Streaming: the renditions of a master playlist fed segment by segment into `<video>` through
//! a media source, the rendition chosen by [`crate::abr`] or by the viewer.
//!
//! What to fetch is read from what is buffered, never remembered: the next segment of a track is
//! the first whose middle lies past the buffered range around the playhead, or the one under the
//! playhead when nothing is buffered there. Seeking, eviction and a change of quality are thus
//! all the same case. Every rendition has its keyframes at the same times, so the pictures switch
//! rendition at any segment without a seam. Without a media source, Safari plays the playlist
//! itself; without either, the player says the video cannot play here.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use web_sys::{HtmlVideoElement, MediaSource, MediaSourceReadyState, SourceBuffer};

use crate::abr::{self, Conditions, Estimator};
use crate::playlist::{self, MediaPlaylist, NANOS, Variant};
use crate::state::Quality;
use crate::web::{self, JsResult, error};

/// Seconds of media to buffer ahead of the playhead.
const AHEAD: f64 = 30.0;
/// Seconds of media to keep behind the playhead once more than twice as much is there.
const BEHIND: f64 = 30.0;
/// Seconds by which a buffered range may miss the playhead and still count as under it.
const SLACK: f64 = 0.1;

/// What the engine tells the controls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Report {
    /// The heights of the renditions this browser plays, lowest first; empty when the browser
    /// streams by itself and chooses alone.
    Renditions(Vec<u32>),
    /// The height of the rendition now being fetched.
    Height(u32),
    /// Nothing here can play the video.
    Unplayable,
}

struct Shared {
    video: HtmlVideoElement,
    stopped: Cell<bool>,
    quality: Cell<Quality>,
    /// Set when the viewer changed quality: drop the pictures buffered past the current segment.
    flush: Cell<bool>,
    report: Box<dyn Fn(Report)>,
    url: RefCell<Option<String>>,
}

/// A running stream into one `<video>`.
pub struct Engine {
    shared: Rc<Shared>,
}

impl Engine {
    /// Start streaming the renditions of the master playlist at `master` into `video`.
    pub fn start(
        video: HtmlVideoElement,
        master: String,
        report: impl Fn(Report) + 'static,
    ) -> Engine {
        let shared = Rc::new(Shared {
            video,
            stopped: Cell::new(false),
            quality: Cell::new(Quality::Auto),
            flush: Cell::new(false),
            report: Box::new(report),
            url: RefCell::new(None),
        });
        let running = Rc::clone(&shared);
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(problem) = stream(&running, &master).await {
                web::warn(&problem);
            }
        });
        Engine { shared }
    }

    /// Fetch `quality` from the next segment on.
    pub fn set_quality(&self, quality: Quality) {
        if self.shared.quality.replace(quality) != quality {
            self.shared.flush.set(true);
        }
    }

    /// Stop fetching, and let the browser free the media source.
    pub fn stop(&self) {
        self.shared.stopped.set(true);
        if let Some(url) = self.shared.url.borrow_mut().take() {
            let _ = web_sys::Url::revoke_object_url(&url);
        }
    }
}

/// One source buffer and the playlists that fill it, one per rendition.
struct Track {
    buffer: SourceBuffer,
    playlists: Vec<MediaPlaylist>,
    /// The MIME type of each rendition in this buffer.
    mimes: Vec<String>,
    /// The rendition whose initialization section the buffer holds.
    loaded: Option<usize>,
    /// The segment appended last, so that a buffer ending short of it does not refetch it.
    last: Option<usize>,
}

impl Track {
    /// The buffered range under `time` (seconds), if any.
    fn range_at(&self, time: f64) -> Option<(f64, f64)> {
        web::buffered(&self.buffer)
            .into_iter()
            .find(|(start, end)| *start - SLACK <= time && time <= *end + SLACK)
    }

    /// Seconds buffered ahead of `time`.
    fn ahead(&self, time: f64) -> f64 {
        self.range_at(time)
            .map_or(0.0, |(_, end)| (end - time).max(0.0))
    }

    /// The segment of `rendition` to fetch next for the playhead at `time`.
    fn next(&self, rendition: usize, time: f64) -> Option<usize> {
        let playlist = self.playlists.get(rendition)?;
        let mut next = match self.range_at(time) {
            Some((_, end)) => playlist.first_after(nanos(end)),
            None => playlist.segment_at(nanos(time)),
        };
        if self.last == Some(next) {
            next = next.saturating_add(1);
        }
        (next < playlist.segments.len()).then_some(next)
    }
}

/// Seconds as playlist time.
fn nanos(seconds: f64) -> u64 {
    (seconds.max(0.0) * NANOS as f64).round() as u64
}

/// Playlist time as the seconds a media element counts.
fn seconds(nanos: u64) -> f64 {
    nanos as f64 / NANOS as f64
}

async fn stream(shared: &Shared, master_uri: &str) -> JsResult<()> {
    let text = web::fetch_text(master_uri).await?;
    let master = playlist::parse_master(&text, master_uri).map_err(error)?;
    let separate = master.audio.is_some();
    let Some((source, managed)) = web::media_source() else {
        return play_natively(shared, master_uri);
    };
    let video_mime = |variant: &Variant| {
        if separate {
            format!("video/mp4; codecs=\"{}\"", variant.video_codecs())
        } else {
            format!("video/mp4; codecs=\"{}\"", variant.codecs)
        }
    };
    let variants: Vec<Variant> = master
        .variants
        .into_iter()
        .filter(|variant| web::supports(managed, &video_mime(variant)))
        .collect();
    let Some(lowest) = variants.first() else {
        return play_natively(shared, master_uri);
    };
    let audio_mime = format!("audio/mp4; codecs=\"{}\"", lowest.audio_codecs());
    (shared.report)(Report::Renditions(
        variants.iter().map(|variant| variant.height).collect(),
    ));
    // The media source opens while the playlists download.
    let url = web::attach(&shared.video, &source, managed)?;
    *shared.url.borrow_mut() = Some(url);
    let opened = web::next_event(&source, "sourceopen");
    let mut uris: Vec<String> = variants.iter().map(|variant| variant.uri.clone()).collect();
    uris.extend(master.audio.iter().cloned());
    let texts = web::fetch_texts(&uris).await?;
    let mut playlists = texts
        .iter()
        .zip(&uris)
        .map(|(text, uri)| playlist::parse_media(text, uri).map_err(error))
        .collect::<JsResult<Vec<MediaPlaylist>>>()?;
    let audio = if master.audio.is_some() {
        playlists.pop()
    } else {
        None
    };
    opened.await?;
    let duration = playlists.first().map_or(0, MediaPlaylist::duration);
    source.set_duration(seconds(duration));
    let mimes: Vec<String> = variants.iter().map(video_mime).collect();
    let first_mime = mimes.first().cloned().unwrap_or_default();
    let mut tracks = vec![Track {
        buffer: source.add_source_buffer(&first_mime)?,
        playlists,
        mimes,
        loaded: None,
        last: None,
    }];
    if let Some(audio) = audio {
        tracks.push(Track {
            buffer: source.add_source_buffer(&audio_mime)?,
            playlists: vec![audio],
            mimes: vec![audio_mime],
            loaded: None,
            last: None,
        });
    }
    let heights: Vec<u32> = variants.iter().map(|variant| variant.height).collect();
    let bandwidths: Vec<u64> = variants.iter().map(|variant| variant.bandwidth).collect();
    Feeder {
        shared,
        source,
        tracks,
        heights,
        bandwidths,
        estimator: Estimator::default(),
        rendition: 0,
        reported: None,
        failures: 0,
    }
    .run()
    .await
}

fn play_natively(shared: &Shared, master_uri: &str) -> JsResult<()> {
    if web::plays_hls(&shared.video) {
        (shared.report)(Report::Renditions(Vec::new()));
        shared.video.set_src(master_uri);
    } else {
        (shared.report)(Report::Unplayable);
    }
    Ok(())
}

/// The loop that keeps the buffers filled.
struct Feeder<'a> {
    shared: &'a Shared,
    source: MediaSource,
    /// The pictures first, then the sound when it comes apart.
    tracks: Vec<Track>,
    heights: Vec<u32>,
    bandwidths: Vec<u64>,
    estimator: Estimator,
    /// The rendition of the pictures being fetched.
    rendition: usize,
    reported: Option<u32>,
    failures: u32,
}

impl Feeder<'_> {
    async fn run(mut self) -> JsResult<()> {
        while !self.shared.stopped.get() {
            if self.shared.video.error().is_some() {
                (self.shared.report)(Report::Unplayable);
                return Ok(());
            }
            match self.step().await {
                Ok(true) => self.failures = 0,
                Ok(false) => web::sleep(250).await,
                Err(problem) => {
                    web::warn(&problem);
                    self.failures = self.failures.saturating_add(1);
                    let wait = 500_i32.saturating_mul(1 << self.failures.min(4));
                    if web::is_quota(&problem) {
                        self.trim(true).await?;
                    }
                    web::sleep(wait).await;
                }
            }
        }
        Ok(())
    }

    /// Fetch and append one segment if any is wanted; whether one was.
    async fn step(&mut self) -> JsResult<bool> {
        let time = self.shared.video.current_time();
        if self.shared.flush.replace(false) {
            self.flush(time).await?;
        }
        self.rendition = self.choose(time);
        let wanted = self
            .tracks
            .iter()
            .enumerate()
            .filter_map(|(index, track)| {
                let rendition = if index == 0 { self.rendition } else { 0 };
                let ahead = track.ahead(time);
                let next = track.next(rendition, time)?;
                (ahead < AHEAD).then_some((ahead, index, rendition, next))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let Some((_, index, rendition, segment)) = wanted else {
            self.finish(time).await?;
            return Ok(false);
        };
        if index == 0 {
            let height = self.heights.get(rendition).copied().unwrap_or(0);
            if self.reported.replace(height) != Some(height) {
                (self.shared.report)(Report::Height(height));
            }
        }
        self.trim(false).await?;
        let track = self.tracks.get(index).ok_or_else(|| error("no track"))?;
        let playlist = track
            .playlists
            .get(rendition)
            .ok_or_else(|| error("no rendition"))?;
        let media = playlist
            .segments
            .get(segment)
            .map(|segment| segment.resource.clone())
            .ok_or_else(|| error("no segment"))?;
        let switching = track.loaded != Some(rendition);
        if switching && let Some(mime) = track.mimes.get(rendition) {
            web::change_type(&track.buffer, mime);
        }
        // A rendition's initialization section goes in first; when it lies right before the
        // segment in the same file, as at the start, one request fetches both.
        let fetched = match (&playlist.init, switching) {
            (Some(init), true) => match init.joined(&media) {
                Some(both) => both,
                None => {
                    let init = init.clone();
                    let bytes = web::fetch_bytes(&init).await?;
                    let track = self.tracks.get(index).ok_or_else(|| error("no track"))?;
                    web::append(&track.buffer, &bytes).await?;
                    media
                }
            },
            _ => media,
        };
        let started = web::now();
        let bytes = web::fetch_bytes(&fetched).await?;
        self.estimator
            .sample(u64::from(bytes.byte_length()), web::now() - started);
        if self.shared.stopped.get() {
            return Ok(false);
        }
        let track = self
            .tracks
            .get_mut(index)
            .ok_or_else(|| error("no track"))?;
        web::append(&track.buffer, &bytes).await?;
        track.loaded = Some(rendition);
        track.last = Some(segment);
        Ok(true)
    }

    /// The rendition of the pictures to fetch next.
    fn choose(&self, time: f64) -> usize {
        let last = self.heights.len().saturating_sub(1);
        match self.shared.quality.get() {
            Quality::Height(height) => self.heights.iter().rposition(|h| *h <= height).unwrap_or(0),
            Quality::Auto => {
                let video = &self.shared.video;
                let ratio = web::window().map_or(1.0, |window| window.device_pixel_ratio());
                let shown = (f64::from(video.client_height().max(0)) * ratio).round() as u32;
                let conditions = Conditions {
                    bandwidth: self.estimator.estimate(),
                    buffered: self.tracks.first().map_or(0.0, |track| track.ahead(time)),
                    shown_height: shown,
                    current: self.rendition,
                };
                abr::choose(&self.bandwidths, &self.heights, conditions).min(last)
            }
        }
    }

    /// Drop the pictures buffered past the segment under `time`, so a new quality shows soon.
    async fn flush(&mut self, time: f64) -> JsResult<()> {
        let Some(track) = self.tracks.first_mut() else {
            return Ok(());
        };
        let Some(playlist) = track.playlists.get(track.loaded.unwrap_or(0)) else {
            return Ok(());
        };
        let end = playlist
            .segments
            .get(playlist.segment_at(nanos(time)))
            .map_or(0, |segment| segment.end());
        let duration = self.source.duration();
        if duration.is_finite() {
            web::remove(&track.buffer, seconds(end), duration).await?;
        }
        track.last = None;
        Ok(())
    }

    /// Drop what is far behind the playhead; with `everything`, all but the last moments.
    async fn trim(&mut self, everything: bool) -> JsResult<()> {
        let time = self.shared.video.current_time();
        let keep = if everything { 2.0 } else { BEHIND };
        for track in &self.tracks {
            let start = web::buffered(&track.buffer)
                .first()
                .map_or(time, |range| range.0);
            if everything || start < time - 2.0 * BEHIND {
                web::remove(&track.buffer, 0.0, time - keep).await?;
            }
        }
        Ok(())
    }

    /// Mark the stream ended once every track holds its last segment.
    async fn finish(&self, time: f64) -> JsResult<()> {
        let open = self.source.ready_state() == MediaSourceReadyState::Open;
        let complete = self.tracks.iter().all(|track| {
            track
                .playlists
                .get(track.loaded.unwrap_or(0))
                .is_some_and(|playlist| {
                    track.range_at(time).is_some_and(|(_, end)| {
                        playlist.first_after(nanos(end)) >= playlist.segments.len()
                    })
                })
        });
        if open && complete && self.tracks.iter().all(|track| !track.buffer.updating()) {
            self.source.end_of_stream()?;
        }
        Ok(())
    }
}
