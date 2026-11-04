//! Streaming: the renditions of a master playlist fed segment by segment into `<video>` through
//! a media source, the rendition chosen by [`crate::abr`] or by the viewer.
//!
//! What to fetch is read from what is buffered, by [`crate::schedule`], never remembered, so
//! seeking, eviction and a change of quality are all one case. Every rendition has its keyframes
//! at the same times, so the pictures switch rendition at any segment without a seam. A fetch has
//! a deadline in proportion to the media it brings, and a seek or a choice of quality that leaves
//! it unwanted abandons it at once. When the browser has no room left, what lies behind the
//! playhead goes first, then the target ahead shrinks. Without a media source, Safari plays the
//! playlist itself; without either, or when the playlists cannot be had, the player says so.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use js_sys::ArrayBuffer;
use wasm_bindgen::JsValue;
use web_sys::{HtmlVideoElement, MediaSource, MediaSourceReadyState, SourceBuffer};
use youcraft_api::time::MediaDuration;

use crate::abr::{self, Conditions, Estimator};
use crate::cmaf;
use crate::playlist::{self, MediaPlaylist, Variant};
use crate::schedule::{self, Want};
use crate::state::Quality;
use crate::time::{self, NANOS};
use crate::web::{self, Alarm, Fetch, JsResult, SMALL_DEADLINE, Wait, error};

/// Nanoseconds of media to buffer ahead of the playhead, while the browser has room.
const AHEAD: u64 = 30 * NANOS;
/// The least the target ahead shrinks to when the browser has no room left.
const LEAST_AHEAD: u64 = 6 * NANOS;
/// Nanoseconds of media to keep behind the playhead once more than twice as much is there.
const BEHIND: u64 = 30 * NANOS;
/// What stays behind the playhead when the browser has no room left.
const LEAST_BEHIND: u64 = 2 * NANOS;
/// Milliseconds between two looks at the buffers while nothing is wanted; a seek, a stall or a
/// choice of quality looks at once.
const LOOK_EVERY: i32 = 250;
/// A segment may take this many times as long as it lasts to arrive, plus [`DEADLINE_BASE`]:
/// slower than that, the network cannot carry its rendition, and the estimate learns so.
const DEADLINE_PER_MEDIA: u64 = 4;
/// Milliseconds every segment may take besides.
const DEADLINE_BASE: u64 = 4000;
/// How many times a playlist is fetched before the player gives up, waiting 1, 2, then 4
/// seconds between tries while the failure may pass.
const TRIES: u32 = 4;
/// Bytes of a fetch read for how long a frame lasts: enough for its initialization section and
/// the boxes that head its first fragment, which take 16 bytes a frame.
const HEAD: u32 = 16 * 1024;

/// What the engine tells the controls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Report {
    /// The lines of the renditions this browser plays (as "720p" counts them), lowest first;
    /// empty when the browser streams by itself and chooses alone.
    Renditions(Vec<u32>),
    /// How long a frame lasts: as the playlist writes it first, exactly once the stream says.
    Frame(MediaDuration),
    /// Nothing here can play the video.
    Unplayable,
    /// The video's playlists could not be had or read.
    Failed,
}

struct Shared {
    video: HtmlVideoElement,
    stopped: Cell<bool>,
    quality: Cell<Quality>,
    /// Set when the viewer chose another quality, until the feeder has acted on it.
    chosen: Cell<bool>,
    /// Ends the feeder's wait at once.
    alarm: Alarm,
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
            chosen: Cell::new(false),
            alarm: Alarm::default(),
            report: Box::new(report),
            url: RefCell::new(None),
        });
        let running = Rc::clone(&shared);
        wasm_bindgen_futures::spawn_local(async move { stream(&running, &master).await });
        Engine { shared }
    }

    /// Fetch `quality` from now on.
    pub fn set_quality(&self, quality: Quality) {
        if self.shared.quality.replace(quality) != quality {
            self.shared.chosen.set(true);
            self.shared.alarm.ring();
        }
    }

    /// Stop fetching, and let the browser free the media source.
    pub fn stop(&self) {
        self.shared.stopped.set(true);
        self.shared.alarm.ring();
        if let Some(url) = self.shared.url.borrow_mut().take() {
            let _ = web_sys::Url::revoke_object_url(&url);
        }
    }
}

async fn stream(shared: &Shared, master_uri: &str) {
    match open(shared, master_uri).await {
        Ok(Some(feeder)) => feeder.run().await,
        Ok(None) => {}
        Err(problem) => {
            web::warn(&problem);
            if !shared.stopped.get() {
                (shared.report)(Report::Failed);
            }
        }
    }
}

/// The text at `uri`, fetched again after growing waits while the failure may pass.
async fn text(uri: String) -> JsResult<JsValue> {
    let mut tries = 1;
    loop {
        match web::fetch_text(&uri, SMALL_DEADLINE).await {
            Ok(text) => return Ok(JsValue::from(text)),
            Err(problem) if tries < TRIES && web::is_transient(&problem) => {
                web::warn(&problem);
                let pause = 1000_i32.saturating_mul(1 << (tries - 1));
                web::sleep(pause).await?;
                tries += 1;
            }
            Err(problem) => return Err(problem),
        }
    }
}

/// `value` as the text it is.
fn as_text(value: &JsValue) -> JsResult<String> {
    value.as_string().ok_or_else(|| error("not text"))
}

/// Read the playlists and open a media source for them: the feeder that will fill it, or `None`
/// when the browser plays the video by itself, or cannot.
async fn open<'a>(shared: &'a Shared, master_uri: &str) -> JsResult<Option<Feeder<'a>>> {
    let text_of_master = as_text(&text(master_uri.to_owned()).await?)?;
    let master = playlist::parse_master(&text_of_master, master_uri).map_err(error)?;
    if let Some(frame) = master.variants.first().and_then(|variant| variant.frame) {
        (shared.report)(Report::Frame(frame));
    }
    let Some((source, managed)) = web::media_source() else {
        play_natively(shared, master_uri);
        return Ok(None);
    };
    let separate = master.audio.is_some();
    let video_mime = |variant: &Variant| {
        let codecs = if separate {
            variant.video_codecs()
        } else {
            variant.codecs.clone()
        };
        format!("video/mp4; codecs=\"{codecs}\"")
    };
    let variants: Vec<Variant> = master
        .variants
        .iter()
        .filter(|variant| web::supports(managed, &video_mime(variant)))
        .cloned()
        .collect();
    let audio_mime = variants
        .first()
        .map(|lowest| format!("audio/mp4; codecs=\"{}\"", lowest.audio_codecs()));
    let sound_plays = !separate
        || audio_mime
            .as_ref()
            .is_some_and(|mime| web::supports(managed, mime));
    if variants.is_empty() || !sound_plays {
        play_natively(shared, master_uri);
        return Ok(None);
    }
    (shared.report)(Report::Renditions(
        variants.iter().map(Variant::lines).collect(),
    ));
    // The media source opens while the playlists download.
    let url = web::attach(&shared.video, &source, managed)?;
    *shared.url.borrow_mut() = Some(url);
    let mut uris: Vec<String> = variants.iter().map(|variant| variant.uri.clone()).collect();
    uris.extend(master.audio.iter().cloned());
    let texts = web::all(uris.iter().cloned().map(text).collect()).await?;
    let mut playlists = texts
        .iter()
        .zip(&uris)
        .map(|(text, uri)| playlist::parse_media(&as_text(text)?, uri).map_err(error))
        .collect::<JsResult<Vec<MediaPlaylist>>>()?;
    let audio = if separate { playlists.pop() } else { None };
    if shared.stopped.get() {
        return Ok(None);
    }
    if source.ready_state() != MediaSourceReadyState::Open {
        Wait::new(&source, &["sourceopen"], None, Some(SMALL_DEADLINE))?
            .done()
            .await;
        if source.ready_state() != MediaSourceReadyState::Open {
            return Err(error("the media source did not open"));
        }
    }
    let duration = playlists.first().map_or(0, MediaPlaylist::duration);
    source.set_duration(time::seconds(duration));
    let mimes: Vec<String> = variants.iter().map(video_mime).collect();
    let first_mime = mimes.first().cloned().unwrap_or_default();
    let mut tracks = vec![Track::new(
        source.add_source_buffer(&first_mime)?,
        playlists,
        mimes,
    )];
    if let (Some(audio), Some(mime)) = (audio, audio_mime) {
        tracks.push(Track::new(
            source.add_source_buffer(&mime)?,
            vec![audio],
            vec![mime],
        ));
    }
    Ok(Some(Feeder {
        shared,
        source,
        tracks,
        variants,
        estimator: Estimator::default(),
        rendition: 0,
        target: AHEAD,
        framed: false,
        failures: 0,
    }))
}

fn play_natively(shared: &Shared, master_uri: &str) {
    if web::plays_hls(&shared.video) {
        (shared.report)(Report::Renditions(Vec::new()));
        shared.video.set_src(master_uri);
    } else {
        (shared.report)(Report::Unplayable);
    }
}

/// Milliseconds a segment lasting `duration` nanoseconds may take to arrive.
fn deadline(duration: u64) -> i32 {
    let millis = (duration / 1_000_000)
        .saturating_mul(DEADLINE_PER_MEDIA)
        .saturating_add(DEADLINE_BASE);
    i32::try_from(millis).unwrap_or(i32::MAX)
}

/// One source buffer and the playlists that fill it, one per rendition.
struct Track {
    buffer: SourceBuffer,
    playlists: Vec<MediaPlaylist>,
    /// The MIME type of each rendition in this buffer.
    mimes: Vec<String>,
    /// The rendition whose initialization section the buffer holds.
    loaded: Option<usize>,
    /// The segment appended last.
    last: Option<usize>,
}

impl Track {
    fn new(buffer: SourceBuffer, playlists: Vec<MediaPlaylist>, mimes: Vec<String>) -> Track {
        Track {
            buffer,
            playlists,
            mimes,
            loaded: None,
            last: None,
        }
    }

    /// What it wants for the playhead at `time`, fed from `rendition`.
    fn want(&self, rendition: usize, time: u64) -> Want {
        let ranges = web::buffered(&self.buffer);
        self.playlists.get(rendition).map_or(
            Want {
                ahead: 0,
                next: None,
                complete: true,
            },
            |playlist| schedule::want(playlist, &ranges, time, self.last),
        )
    }
}

/// The loop that keeps the buffers filled.
struct Feeder<'a> {
    shared: &'a Shared,
    source: MediaSource,
    /// The pictures first, then the sound when it comes apart.
    tracks: Vec<Track>,
    /// The renditions of the pictures, lowest first.
    variants: Vec<Variant>,
    estimator: Estimator,
    /// The rendition of the pictures being fetched.
    rendition: usize,
    /// Nanoseconds to buffer ahead: [`AHEAD`], or less once the browser had no room left.
    target: u64,
    /// Whether the stream has told how long a frame lasts.
    framed: bool,
    failures: u32,
}

impl Feeder<'_> {
    async fn run(mut self) {
        while !self.shared.stopped.get() {
            if self.shared.video.error().is_some() {
                (self.shared.report)(Report::Unplayable);
                return;
            }
            let pause = match self.step().await {
                Ok(true) => {
                    self.failures = 0;
                    continue;
                }
                Ok(false) => LOOK_EVERY,
                Err(problem) => {
                    web::warn(&problem);
                    self.failures = self.failures.saturating_add(1);
                    500_i32.saturating_mul(1 << self.failures.min(4))
                }
            };
            let wait = Wait::new(
                &self.shared.video,
                &["seeking", "waiting"],
                Some(&self.shared.alarm),
                Some(pause),
            );
            match wait {
                Ok(wait) => wait.done().await,
                Err(problem) => {
                    // Without a way to wait, looking again at once would never stop.
                    web::warn(&problem);
                    return;
                }
            }
        }
    }

    /// Where the playhead is.
    fn playhead(&self) -> u64 {
        time::from_seconds(self.shared.video.current_time())
    }

    /// The rendition track `index` is fed from: the chosen one for the pictures, the only one
    /// for the sound.
    fn rendition_of(&self, index: usize) -> usize {
        if index == 0 { self.rendition } else { 0 }
    }

    /// Fetch and append one segment if any is wanted; whether to look again at once, as after
    /// an append, or a fetch abandoned for what a seek or a choice of quality wants instead.
    async fn step(&mut self) -> JsResult<bool> {
        let time = self.playhead();
        let rendition = self.choose(time);
        if self.shared.chosen.replace(false) && rendition != self.rendition {
            self.flush(time).await?;
        }
        self.rendition = rendition;
        let wants: Vec<Want> = self
            .tracks
            .iter()
            .enumerate()
            .map(|(index, track)| track.want(self.rendition_of(index), time))
            .collect();
        let Some((index, segment)) = schedule::neediest(&wants, self.target) else {
            self.finish(&wants)?;
            return Ok(false);
        };
        self.trim(time).await;
        let rendition = self.rendition_of(index);
        let track = self.tracks.get(index).ok_or_else(|| error("no track"))?;
        let playlist = track
            .playlists
            .get(rendition)
            .ok_or_else(|| error("no rendition"))?;
        let piece = playlist
            .segments
            .get(segment)
            .ok_or_else(|| error("no segment"))?;
        let (media, duration) = (piece.resource.clone(), piece.duration);
        let switching = track.loaded != Some(rendition);
        let mime = track.mimes.get(rendition).cloned();
        let init = playlist.init.clone().filter(|_| switching);
        // A rendition's initialization section goes in first; when it lies right before the
        // segment in the same file, as at the start, one request fetches both. The buffer takes
        // the rendition's type with the first bytes of it, and what a switch brings first tells
        // how long a frame lasts.
        let mut begins = mime.as_deref().filter(|_| switching);
        let reading = index == 0 && switching && !self.framed;
        let mut head = Vec::new();
        let resource = match init {
            Some(init) => match init.joined(&media) {
                Some(both) => both,
                None => {
                    let bytes = Fetch::bytes(&init, SMALL_DEADLINE)?.done().await?;
                    if reading {
                        head = web::prefix(&bytes, HEAD);
                    }
                    self.append(index, &bytes, begins.take(), time).await?;
                    if let Some(track) = self.tracks.get_mut(index) {
                        track.loaded = Some(rendition);
                    }
                    media
                }
            },
            None => media,
        };
        let Some(bytes) = self.fetch(index, segment, &resource, duration).await? else {
            return Ok(true);
        };
        if reading {
            head.extend(web::prefix(&bytes, HEAD));
        }
        self.append(index, &bytes, begins, time).await?;
        // Read once the bytes are in, so that the first picture waits for nothing more.
        if reading && let Some(frame) = cmaf::frame_duration(&head) {
            (self.shared.report)(Report::Frame(frame));
            self.framed = true;
        }
        let track = self
            .tracks
            .get_mut(index)
            .ok_or_else(|| error("no track"))?;
        track.loaded = Some(rendition);
        track.last = Some(segment);
        Ok(true)
    }

    /// The bytes of `resource`, segment `segment` of track `index` lasting `duration`; `None`
    /// once a seek, a choice of quality or the end of the stream leaves them unwanted.
    async fn fetch(
        &mut self,
        index: usize,
        segment: usize,
        resource: &playlist::Resource,
        duration: u64,
    ) -> JsResult<Option<ArrayBuffer>> {
        let started = web::now();
        let fetch = Fetch::bytes(resource, deadline(duration))?;
        loop {
            let woken = Wait::new(
                &self.shared.video,
                &["seeking"],
                Some(&self.shared.alarm),
                None,
            )?;
            match fetch.before(&woken).await {
                Ok(Some(bytes)) => {
                    self.estimator
                        .sample(u64::from(bytes.byte_length()), web::now() - started);
                    return Ok(Some(bytes));
                }
                Ok(None) => {
                    if self.shared.stopped.get() || !self.still_wanted(index, segment) {
                        fetch.abort();
                        return Ok(None);
                    }
                }
                Err(problem) => {
                    // Abandoned at its deadline: the network carries less than this much.
                    if web::is_abandoned(&problem)
                        && let Some(range) = resource.range
                    {
                        self.estimator.sample(range.length, web::now() - started);
                    }
                    return Err(problem);
                }
            }
        }
    }

    /// Whether segment `segment` of track `index` is what that track still wants, after a seek
    /// or a choice of quality.
    fn still_wanted(&self, index: usize, segment: usize) -> bool {
        let time = self.playhead();
        let rendition = if index == 0 && self.shared.chosen.get() {
            self.choose(time)
        } else {
            self.rendition_of(index)
        };
        rendition == self.rendition_of(index)
            && self
                .tracks
                .get(index)
                .is_some_and(|track| track.want(rendition, time).next == Some(segment))
    }

    /// Append `bytes` to track `index`, its buffer first told to take `mime` when the bytes
    /// begin another rendition. Out of room, make some, and fail so that the step is tried again.
    async fn append(
        &mut self,
        index: usize,
        bytes: &ArrayBuffer,
        mime: Option<&str>,
        time: u64,
    ) -> JsResult<()> {
        let buffer = self
            .tracks
            .get(index)
            .ok_or_else(|| error("no track"))?
            .buffer
            .clone();
        if let Some(mime) = mime {
            web::change_type(&buffer, mime);
        }
        let appended = web::append(&buffer, bytes).await;
        if let Err(problem) = &appended
            && web::is_quota(problem)
        {
            self.make_room(&buffer, time).await;
        }
        appended
    }

    /// Room in `buffer`, which had none left: what lies behind the playhead at `time` goes, or,
    /// with nothing there, the target ahead shrinks to what the buffer holds.
    async fn make_room(&mut self, buffer: &SourceBuffer, time: u64) {
        let ranges = web::buffered(buffer);
        if let Some(cut) = schedule::stale(&ranges, time, LEAST_BEHIND, LEAST_BEHIND) {
            if let Err(problem) = web::remove(buffer, 0, cut).await {
                web::warn(&problem);
            }
            return;
        }
        let ahead =
            schedule::range_at(&ranges, time).map_or(0, |(_, end)| end.saturating_sub(time));
        self.target = self.target.min(ahead).max(LEAST_AHEAD);
    }

    /// The rendition of the pictures to fetch next.
    fn choose(&self, time: u64) -> usize {
        let last = self.variants.len().saturating_sub(1);
        match self.shared.quality.get() {
            Quality::Lines(lines) => self
                .variants
                .iter()
                .rposition(|variant| variant.lines() <= lines)
                .unwrap_or(0),
            Quality::Auto => {
                let video = &self.shared.video;
                let ratio = web::window().map_or(1.0, |window| window.device_pixel_ratio());
                let device = |css: i32| (f64::from(css.max(0)) * ratio).round() as u32;
                let conditions = Conditions {
                    bandwidth: self.estimator.estimate(),
                    buffered: self
                        .tracks
                        .first()
                        .map_or(0, |track| track.want(self.rendition, time).ahead),
                    shown: (device(video.client_width()), device(video.client_height())),
                    current: self.rendition,
                };
                abr::choose(&self.variants, conditions).min(last)
            }
        }
    }

    /// Drop the pictures buffered past the segment under the playhead at `time`, so that the
    /// viewer's new choice of quality shows soon.
    async fn flush(&mut self, time: u64) -> JsResult<()> {
        let end = time::from_seconds(self.source.duration());
        let Some(track) = self.tracks.first_mut() else {
            return Ok(());
        };
        let Some(playlist) = track.playlists.get(track.loaded.unwrap_or(0)) else {
            return Ok(());
        };
        web::remove(&track.buffer, schedule::flush_from(playlist, time), end).await?;
        track.last = None;
        Ok(())
    }

    /// Drop what lies far behind the playhead at `time`.
    async fn trim(&self, time: u64) {
        for track in &self.tracks {
            let ranges = web::buffered(&track.buffer);
            if let Some(cut) = schedule::stale(&ranges, time, BEHIND, 2 * BEHIND)
                && let Err(problem) = web::remove(&track.buffer, 0, cut).await
            {
                web::warn(&problem);
            }
        }
    }

    /// Mark the stream ended once every track holds everything to the end.
    fn finish(&self, wants: &[Want]) -> JsResult<()> {
        let open = self.source.ready_state() == MediaSourceReadyState::Open;
        let idle = self.tracks.iter().all(|track| !track.buffer.updating());
        if open && idle && wants.iter().all(|want| want.complete) {
            self.source.end_of_stream()?;
        }
        Ok(())
    }
}
