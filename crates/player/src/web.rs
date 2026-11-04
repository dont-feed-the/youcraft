//! The browser calls the player makes, as Rust futures and `Result`s: fetching bytes and text
//! within a deadline, waiting for events, a delay or the engine's alarm, and opening a media
//! source. Times cross here between the browser's floating-point seconds and the player's
//! nanoseconds.
//!
//! Nothing here hands the browser a Rust closure: waits resolve a promise's own `resolve` from
//! listeners that an `AbortSignal` removes, and deadlines call `AbortController.abort` itself, so
//! nothing leaks however a wait or a fetch ends. iPhone Safari offers only `ManagedMediaSource`, a
//! `MediaSource` the system may evict from, and no global `MediaSource` to check instances
//! against; it is therefore found by name and used through `MediaSource`'s methods.

use std::cell::RefCell;

use js_sys::{Array, ArrayBuffer, Function, Promise, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AbortController, AddEventListenerOptions, EventTarget, Headers, HtmlVideoElement, MediaSource,
    Request, RequestInit, Response, SourceBuffer, TimeRanges, Url, Window,
};

use crate::playlist::Resource;
use crate::schedule::Range;
use crate::time;

/// What a failed browser call throws.
pub type JsResult<T> = Result<T, JsValue>;

/// Milliseconds a small document, such as a playlist, a storyboard or an initialization section,
/// may take to arrive.
pub const SMALL_DEADLINE: i32 = 10_000;

/// The name of the errors a server's refusal throws.
const REFUSED: &str = "HttpError";

/// An error of our own, as browser calls throw them.
pub fn error(message: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&message.to_string())
}

/// The window.
pub fn window() -> JsResult<Window> {
    web_sys::window().ok_or_else(|| error("no window"))
}

/// Milliseconds since the page started, for timing downloads.
pub fn now() -> f64 {
    window()
        .ok()
        .and_then(|window| window.performance())
        .map_or(0.0, |performance| performance.now())
}

/// What wakes the [`Wait`] in progress from elsewhere in the program, as a seek's event does.
#[derive(Default)]
pub struct Alarm {
    ring: RefCell<Option<Function>>,
}

impl Alarm {
    /// End the wait in progress, if any.
    pub fn ring(&self) {
        let resolve = self.ring.borrow_mut().take();
        if let Some(resolve) = resolve {
            let _ = resolve.call0(&JsValue::UNDEFINED);
        }
    }
}

/// A wait that ends at the first of some events of a target, a delay and an alarm; its
/// listeners and its timer go with it.
pub struct Wait {
    promise: Promise,
    listening: AbortController,
    timer: Option<i32>,
}

impl Wait {
    /// Wait for the first of the events `names` of `target`, `millis` milliseconds when given,
    /// and `alarm` when given; listening starts now, so an event that fires while something else
    /// is awaited ends the wait.
    pub fn new(
        target: &EventTarget,
        names: &[&str],
        alarm: Option<&Alarm>,
        millis: Option<i32>,
    ) -> JsResult<Wait> {
        let mut resolver = None;
        let promise = Promise::new(&mut |resolve, _| resolver = Some(resolve));
        let resolve = resolver.ok_or_else(|| error("a promise without resolve"))?;
        let listening = AbortController::new()?;
        let options = AddEventListenerOptions::new();
        options.set_once(true);
        options.set_signal(&listening.signal());
        for name in names {
            target.add_event_listener_with_callback_and_add_event_listener_options(
                name, &resolve, &options,
            )?;
        }
        let timer = match millis {
            Some(millis) => Some(
                window()?
                    .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, millis)?,
            ),
            None => None,
        };
        if let Some(alarm) = alarm {
            *alarm.ring.borrow_mut() = Some(resolve);
        }
        Ok(Wait {
            promise,
            listening,
            timer,
        })
    }

    /// Wait until it ends.
    pub async fn done(&self) {
        let _ = JsFuture::from(self.promise.clone()).await;
    }
}

impl Drop for Wait {
    fn drop(&mut self) {
        self.listening.abort();
        if let (Some(timer), Ok(window)) = (self.timer, window()) {
            window.clear_timeout_with_handle(timer);
        }
    }
}

/// Wait `millis` milliseconds.
pub async fn sleep(millis: i32) -> JsResult<()> {
    let window = window()?;
    Wait::new(&window, &[], None, Some(millis))?.done().await;
    Ok(())
}

/// A fetch under way, which its deadline or its owner may abandon.
pub struct Fetch {
    promise: Promise,
    controller: AbortController,
    timer: i32,
}

impl Fetch {
    /// Start fetching exactly the bytes of `resource`, abandoned after `deadline` milliseconds:
    /// a server that answers a range with the whole file is an error, not a buffer full of the
    /// wrong bytes.
    pub fn bytes(resource: &Resource, deadline: i32) -> JsResult<Fetch> {
        let headers = Headers::new()?;
        if let Some(range) = resource.range {
            headers.set("Range", &range.header())?;
        }
        let (response, controller, timer) = request(&resource.uri, &headers, deadline)?;
        let (uri, length) = (
            resource.uri.clone(),
            resource.range.map(|range| range.length),
        );
        let promise = wasm_bindgen_futures::future_to_promise(async move {
            let response = answered(JsFuture::from(response).await?, &uri)?;
            let bytes: ArrayBuffer = JsFuture::from(response.array_buffer()?).await?.dyn_into()?;
            if length.is_some_and(|length| u64::from(bytes.byte_length()) != length) {
                return Err(error(format!("{uri}: the range came back whole")));
            }
            Ok(bytes.into())
        });
        Ok(Fetch {
            promise,
            controller,
            timer,
        })
    }

    /// Give it up: it fails at once with an `AbortError`.
    pub fn abort(&self) {
        self.controller.abort();
    }

    /// Its bytes if they come before `wait` ends; `None` if `wait` ends first, the fetch going on.
    pub async fn before(&self, wait: &Wait) -> JsResult<Option<ArrayBuffer>> {
        let first = Promise::race(&Array::of2(&self.promise, &wait.promise));
        Ok(JsFuture::from(first).await?.dyn_into::<ArrayBuffer>().ok())
    }

    /// Its bytes.
    pub async fn done(&self) -> JsResult<ArrayBuffer> {
        JsFuture::from(self.promise.clone()).await?.dyn_into()
    }
}

impl Drop for Fetch {
    fn drop(&mut self) {
        if let Ok(window) = window() {
            window.clear_timeout_with_handle(self.timer);
        }
    }
}

/// Start a request for `uri` with `headers`, abandoned after `deadline` milliseconds: the
/// response's promise, what abandons it, and the timer that will.
fn request(
    uri: &str,
    headers: &Headers,
    deadline: i32,
) -> JsResult<(Promise, AbortController, i32)> {
    let controller = AbortController::new()?;
    let init = RequestInit::new();
    init.set_headers(headers);
    init.set_signal(Some(&controller.signal()));
    let request = Request::new_with_str_and_init(uri, &init)?;
    let window = window()?;
    let abort: Function = Reflect::get(&controller, &JsValue::from_str("abort"))?.dyn_into()?;
    let timer = window.set_timeout_with_callback_and_timeout_and_arguments_0(
        &abort.bind0(&controller),
        deadline,
    )?;
    Ok((window.fetch_with_request(&request), controller, timer))
}

/// `value` as a response that succeeded, or the server's refusal as an error that says its
/// status.
fn answered(value: JsValue, uri: &str) -> JsResult<Response> {
    let response: Response = value.dyn_into()?;
    if response.ok() {
        return Ok(response);
    }
    let refusal = js_sys::Error::new(&format!("{uri}: HTTP {}", response.status()));
    refusal.set_name(REFUSED);
    Reflect::set(
        &refusal,
        &JsValue::from_str("status"),
        &JsValue::from(response.status()),
    )?;
    Err(refusal.into())
}

/// The text at `uri`, abandoned after `deadline` milliseconds.
pub async fn fetch_text(uri: &str, deadline: i32) -> JsResult<String> {
    let (response, _controller, timer) = request(uri, &Headers::new()?, deadline)?;
    let text = async {
        let response = answered(JsFuture::from(response).await?, uri)?;
        JsFuture::from(response.text()?)
            .await?
            .as_string()
            .ok_or_else(|| error("not text"))
    }
    .await;
    if let Ok(window) = window() {
        window.clear_timeout_with_handle(timer);
    }
    text
}

/// The values of `futures`, all run at once, in their order.
pub async fn all<F>(futures: Vec<F>) -> JsResult<Vec<JsValue>>
where
    F: Future<Output = JsResult<JsValue>> + 'static,
{
    let pending: Vec<JsFuture> = futures
        .into_iter()
        .map(|future| JsFuture::from(wasm_bindgen_futures::future_to_promise(future)))
        .collect();
    let mut values = Vec::with_capacity(pending.len());
    for value in pending {
        values.push(value.await?);
    }
    Ok(values)
}

/// Whether `problem` may pass if tried again: the network failed, a deadline passed, or the
/// server was briefly unable.
pub fn is_transient(problem: &JsValue) -> bool {
    let name = Reflect::get(problem, &JsValue::from_str("name"))
        .ok()
        .and_then(|name| name.as_string())
        .unwrap_or_default();
    match name.as_str() {
        "TypeError" | "AbortError" | "TimeoutError" => true,
        REFUSED => Reflect::get(problem, &JsValue::from_str("status"))
            .ok()
            .and_then(|status| status.as_f64())
            .is_some_and(|status| status >= 500.0 || status == 408.0 || status == 429.0),
        _ => false,
    }
}

/// Whether `problem` is a fetch abandoned at its deadline.
pub fn is_abandoned(problem: &JsValue) -> bool {
    Reflect::get(problem, &JsValue::from_str("name"))
        .ok()
        .and_then(|name| name.as_string())
        .is_some_and(|name| name == "AbortError")
}

/// Whether `problem` is a buffer refusing bytes for lack of room.
pub fn is_quota(problem: &JsValue) -> bool {
    Reflect::get(problem, &JsValue::from_str("name"))
        .ok()
        .and_then(|name| name.as_string())
        .is_some_and(|name| name == "QuotaExceededError")
}

/// The first `most` bytes of `bytes`, copied out of the browser's memory.
pub fn prefix(bytes: &ArrayBuffer, most: u32) -> Vec<u8> {
    Uint8Array::new_with_byte_offset_and_length(bytes, 0, bytes.byte_length().min(most)).to_vec()
}

/// A media source to stream into, and whether it is a managed one.
pub fn media_source() -> Option<(MediaSource, bool)> {
    let window = window().ok()?;
    let managed = Reflect::get(&window, &JsValue::from_str("ManagedMediaSource"))
        .ok()
        .and_then(|constructor| constructor.dyn_into::<Function>().ok())
        .and_then(|constructor| Reflect::construct(&constructor, &Array::new()).ok());
    if let Some(source) = managed {
        return Some((source.unchecked_into(), true));
    }
    if !Reflect::has(&window, &JsValue::from_str("MediaSource")).unwrap_or(false) {
        return None;
    }
    MediaSource::new().ok().map(|source| (source, false))
}

/// Whether the media source kind (`managed` or not) can play `mime`.
pub fn supports(managed: bool, mime: &str) -> bool {
    if !managed {
        return MediaSource::is_type_supported(mime);
    }
    let Ok(window) = window() else {
        return false;
    };
    let check =
        Reflect::get(&window, &JsValue::from_str("ManagedMediaSource")).and_then(|constructor| {
            let method: Function =
                Reflect::get(&constructor, &JsValue::from_str("isTypeSupported"))?.dyn_into()?;
            method.call1(&constructor, &JsValue::from_str(mime))
        });
    check.is_ok_and(|answer| answer.is_truthy())
}

/// Play `source` in `video`; for a managed source, give up AirPlay, which it requires.
pub fn attach(video: &HtmlVideoElement, source: &MediaSource, managed: bool) -> JsResult<String> {
    if managed {
        Reflect::set(
            video,
            &JsValue::from_str("disableRemotePlayback"),
            &JsValue::TRUE,
        )?;
    }
    let url = Url::create_object_url_with_source(source)?;
    video.set_src(&url);
    Ok(url)
}

/// Whether `video` plays HLS itself, as Safari does.
pub fn plays_hls(video: &HtmlVideoElement) -> bool {
    !video
        .can_play_type("application/vnd.apple.mpegurl")
        .is_empty()
}

/// Let `buffer` take media of another `mime` from now on, where the browser can.
pub fn change_type(buffer: &SourceBuffer, mime: &str) {
    let method = Reflect::get(buffer, &JsValue::from_str("changeType"))
        .ok()
        .and_then(|method| method.dyn_into::<Function>().ok());
    if let Some(method) = method {
        let _ = method.call1(buffer, &JsValue::from_str(mime));
    }
}

/// Milliseconds a buffer may take to take or drop bytes.
const UPDATE_DEADLINE: i32 = 10_000;

/// Wait until `buffer` has finished what it was asked, within [`UPDATE_DEADLINE`].
async fn updated(buffer: &SourceBuffer, wait: Wait) -> JsResult<()> {
    wait.done().await;
    if buffer.updating() {
        return Err(error("the buffer took too long"));
    }
    Ok(())
}

/// Append `bytes` to `buffer` and wait until it took them.
pub async fn append(buffer: &SourceBuffer, bytes: &ArrayBuffer) -> JsResult<()> {
    let wait = Wait::new(buffer, &["updateend"], None, Some(UPDATE_DEADLINE))?;
    buffer.append_buffer_with_array_buffer(bytes)?;
    updated(buffer, wait).await
}

/// Remove `start .. end` (nanoseconds) from `buffer` and wait until it did.
pub async fn remove(buffer: &SourceBuffer, start: u64, end: u64) -> JsResult<()> {
    if end <= start {
        return Ok(());
    }
    let wait = Wait::new(buffer, &["updateend"], None, Some(UPDATE_DEADLINE))?;
    buffer.remove(time::seconds(start), time::seconds(end))?;
    updated(buffer, wait).await
}

/// The time ranges `found`, in nanoseconds.
pub fn ranges(found: &TimeRanges) -> Vec<Range> {
    (0..found.length())
        .filter_map(|index| Some((found.start(index).ok()?, found.end(index).ok()?)))
        .map(|(start, end)| (time::from_seconds(start), time::from_seconds(end)))
        .collect()
}

/// The buffered ranges of `buffer`, in nanoseconds.
pub fn buffered(buffer: &SourceBuffer) -> Vec<Range> {
    buffer
        .buffered()
        .map_or_else(|_| Vec::new(), |found| ranges(&found))
}

/// Whether the browser lets the page fill the screen with an element.
pub fn full_screen_enabled() -> bool {
    window()
        .ok()
        .and_then(|window| window.document())
        .is_some_and(|document| document.fullscreen_enabled())
}

/// Fill the screen with `video` alone, as iPhone Safari does instead of element full screen.
pub fn video_full_screen(video: &HtmlVideoElement) {
    let method = Reflect::get(video, &JsValue::from_str("webkitEnterFullscreen"))
        .ok()
        .and_then(|method| method.dyn_into::<Function>().ok());
    if let Some(method) = method {
        let _ = method.call0(video);
    }
}

/// Write `message` to the console, for whoever debugs the page.
pub fn warn(message: &JsValue) {
    web_sys::console::warn_1(message);
}
