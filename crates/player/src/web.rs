//! The browser calls the player makes, as Rust futures and `Result`s: fetching bytes and text,
//! waiting for an event or a delay, and opening a media source.
//!
//! iPhone Safari offers only `ManagedMediaSource`, a `MediaSource` the system may evict from, and
//! no global `MediaSource` to check instances against; it is therefore found by name and used
//! through `MediaSource`'s methods, which it shares.

use js_sys::{Array, ArrayBuffer, Function, Promise, Reflect};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AddEventListenerOptions, EventTarget, Headers, HtmlVideoElement, MediaSource, Request,
    RequestInit, Response, SourceBuffer, Url, Window,
};

use crate::playlist::Resource;

/// What a failed browser call throws.
pub type JsResult<T> = Result<T, JsValue>;

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

/// Wait `millis` milliseconds.
pub async fn sleep(millis: i32) {
    let promise = Promise::new(&mut |resolve, _| {
        if let Ok(window) = window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, millis);
        }
    });
    let _ = JsFuture::from(promise).await;
}

/// The next `name` event of `target`, listened for from this call on, not from the first poll:
/// an event that fires while something else is awaited is not missed.
pub fn next_event(target: &EventTarget, name: &str) -> impl Future<Output = JsResult<()>> + use<> {
    let promise = Promise::new(&mut |resolve, _| {
        let options = AddEventListenerOptions::new();
        options.set_once(true);
        let _ = target.add_event_listener_with_callback_and_add_event_listener_options(
            name, &resolve, &options,
        );
    });
    let event = JsFuture::from(promise);
    async move { event.await.map(|_| ()) }
}

async fn fetch(uri: &str, init: &RequestInit) -> JsResult<Response> {
    let request = Request::new_with_str_and_init(uri, init)?;
    let response: Response = JsFuture::from(window()?.fetch_with_request(&request))
        .await?
        .dyn_into()?;
    if !response.ok() {
        return Err(error(format!("{uri}: HTTP {}", response.status())));
    }
    Ok(response)
}

/// The text at `uri`.
pub async fn fetch_text(uri: &str) -> JsResult<String> {
    let response = fetch(uri, &RequestInit::new()).await?;
    JsFuture::from(response.text()?)
        .await?
        .as_string()
        .ok_or_else(|| error("not text"))
}

/// The texts at `uris`, all fetched at once, in the order of `uris`.
pub async fn fetch_texts(uris: &[String]) -> JsResult<Vec<String>> {
    let pending: Vec<JsFuture> = uris
        .iter()
        .map(|uri| {
            let uri = uri.clone();
            JsFuture::from(wasm_bindgen_futures::future_to_promise(async move {
                fetch_text(&uri).await.map(JsValue::from)
            }))
        })
        .collect();
    let mut texts = Vec::with_capacity(pending.len());
    for text in pending {
        texts.push(text.await?.as_string().ok_or_else(|| error("not text"))?);
    }
    Ok(texts)
}

/// The bytes of `resource`, exactly the range it names: a server that answers a range with the
/// whole file is an error, not a buffer full of the wrong bytes.
pub async fn fetch_bytes(resource: &Resource) -> JsResult<ArrayBuffer> {
    let init = RequestInit::new();
    if let Some(range) = resource.range {
        let headers = Headers::new()?;
        headers.set("Range", &range.header())?;
        init.set_headers(&headers);
    }
    let response = fetch(&resource.uri, &init).await?;
    let bytes: ArrayBuffer = JsFuture::from(response.array_buffer()?).await?.dyn_into()?;
    if let Some(range) = resource.range
        && u64::from(bytes.byte_length()) != range.length
    {
        return Err(error(format!(
            "{}: the range came back whole",
            resource.uri
        )));
    }
    Ok(bytes)
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

/// Append `bytes` to `buffer` and wait until it took them.
pub async fn append(buffer: &SourceBuffer, bytes: &ArrayBuffer) -> JsResult<()> {
    buffer.append_buffer_with_array_buffer(bytes)?;
    next_event(buffer, "updateend").await
}

/// Remove `start .. end` seconds from `buffer` and wait until it did.
pub async fn remove(buffer: &SourceBuffer, start: f64, end: f64) -> JsResult<()> {
    if end <= start {
        return Ok(());
    }
    buffer.remove(start, end)?;
    next_event(buffer, "updateend").await
}

/// The buffered ranges of `buffer`, in seconds.
pub fn buffered(buffer: &SourceBuffer) -> Vec<(f64, f64)> {
    let Ok(ranges) = buffer.buffered() else {
        return Vec::new();
    };
    (0..ranges.length())
        .filter_map(|index| Some((ranges.start(index).ok()?, ranges.end(index).ok()?)))
        .collect()
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

/// Whether `problem` is a buffer refusing bytes for lack of room.
pub fn is_quota(problem: &JsValue) -> bool {
    Reflect::get(problem, &JsValue::from_str("name"))
        .ok()
        .and_then(|name| name.as_string())
        .is_some_and(|name| name == "QuotaExceededError")
}

/// Write `message` to the console, for whoever debugs the page.
pub fn warn(message: &JsValue) {
    web_sys::console::warn_1(message);
}
