//! The browser's side of an upload: the chosen file, sent piece by piece at its offset; or, for
//! a small file such as a channel's photo, read whole into base64 for a command to carry.
//!
//! Each piece is a slice of the `File` (the browser reads it from disk as it sends, so a file of
//! any size never sits in memory), sent with `PATCH` and the offset it starts at. When the server
//! answers with another offset (a piece that arrived but whose answer was lost), sending resumes
//! from the server's offset. A piece that fails on the way, or an answer that moves nothing
//! forward, is tried again after a wait that doubles each time, a few times at most; a refusal
//! ends the upload with the site's reason. Only the browser has files, so this exists in the
//! `hydrate` build.

use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use youcraft_api::ApiError;
use youcraft_api::records::Upload;

/// How many times in a row a piece may fail before the upload does.
const TRIES: u32 = 6;

/// The wait before the first try again; each next one waits twice as long.
const FIRST_WAIT_MILLISECONDS: i32 = 500;

/// The file chosen in the `<input type=file>` that `event` comes from. The input forgets it, so
/// that choosing the same file again, after a failure or a removal, is a change again.
pub fn chosen_file(event: &web_sys::Event) -> Option<web_sys::File> {
    let input: web_sys::HtmlInputElement = event.target()?.dyn_into().ok()?;
    let file = input.files()?.get(0);
    input.set_value("");
    file
}

/// The bytes of `file`, at most `limit` of them, in base64.
pub async fn base64_of(file: &web_sys::File, limit: usize) -> Result<String, ApiError> {
    use base64::Engine;

    if file.size() > limit as f64 {
        return Err(ApiError::TooLarge {
            limit: u64::try_from(limit).unwrap_or(u64::MAX),
        });
    }
    let buffer = JsFuture::from(file.array_buffer())
        .await
        .map_err(|_| ApiError::Internal)?;
    let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// Ask the person to confirm `question`.
pub fn confirm(question: &str) -> bool {
    web_sys::window()
        .and_then(|w| w.confirm_with_message(question).ok())
        .unwrap_or(false)
}

/// What the site answered to one piece.
enum Answer {
    /// It holds this many bytes now.
    Holds(u64),
    /// It refused the upload, for this reason.
    Refused(ApiError),
    /// The piece or its answer was lost.
    Lost,
}

/// Send `file` to `upload`'s address from its offset on; `sent` hears the bytes received so far.
pub async fn send(
    file: &web_sys::File,
    upload: &Upload,
    sent: impl Fn(u64),
) -> Result<(), ApiError> {
    let window = web_sys::window().ok_or(ApiError::Internal)?;
    if upload.piece == 0 {
        return Err(ApiError::Internal);
    }
    let size = file.size() as u64;
    let mut offset = upload.received;
    let mut failures = 0;
    while offset < size {
        let end = offset.saturating_add(upload.piece).min(size);
        match send_piece(&window, file, upload, offset, end).await? {
            Answer::Holds(held) if held > offset && held <= size => {
                offset = held;
                failures = 0;
                sent(offset);
            }
            Answer::Holds(held) if held > size => return Err(ApiError::Internal),
            Answer::Holds(held) => {
                // The site holds less than was sent: send from where it is, after a wait.
                offset = held;
                failures += 1;
            }
            Answer::Refused(error) => return Err(error),
            Answer::Lost => failures += 1,
        }
        if failures >= TRIES {
            return Err(ApiError::Internal);
        }
        if failures > 0 {
            wait(
                &window,
                FIRST_WAIT_MILLISECONDS.saturating_mul(1 << (failures - 1)),
            )
            .await;
        }
    }
    Ok(())
}

/// Send the bytes `offset..end` of `file`.
async fn send_piece(
    window: &web_sys::Window,
    file: &web_sys::File,
    upload: &Upload,
    offset: u64,
    end: u64,
) -> Result<Answer, ApiError> {
    let piece = file
        .slice_with_f64_and_f64(offset as f64, end as f64)
        .map_err(|_| ApiError::Internal)?;
    let init = web_sys::RequestInit::new();
    init.set_method("PATCH");
    init.set_credentials(web_sys::RequestCredentials::SameOrigin);
    init.set_body(&piece);
    let request = web_sys::Request::new_with_str_and_init(&upload.address, &init)
        .map_err(|_| ApiError::Internal)?;
    let headers = request.headers();
    let _ = headers.set("upload-offset", &offset.to_string());
    let _ = headers.set("content-type", "application/offset+octet-stream");
    let answer = JsFuture::from(window.fetch_with_request(&request)).await;
    let Some(response) = answer
        .ok()
        .and_then(|r| r.dyn_into::<web_sys::Response>().ok())
    else {
        return Ok(Answer::Lost);
    };
    // A conflict says where the site is, as a success does.
    if response.ok() || response.status() == 409 {
        return Ok(offset_of(&response).map_or(Answer::Lost, Answer::Holds));
    }
    // The server failed, or the piece came too slowly for it (408): the piece is tried again.
    if response.status() >= 500 || response.status() == 408 {
        return Ok(Answer::Lost);
    }
    Ok(Answer::Refused(refusal(&response).await))
}

/// The offset the server answered with.
fn offset_of(response: &web_sys::Response) -> Option<u64> {
    response.headers().get("upload-offset").ok()??.parse().ok()
}

/// Why the site refused, as its answer says; what its status says if the answer says nothing.
async fn refusal(response: &web_sys::Response) -> ApiError {
    let said = match response.text() {
        Ok(text) => JsFuture::from(text)
            .await
            .ok()
            .and_then(|text| text.as_string())
            .and_then(|text| serde_json::from_str::<ApiError>(&text).ok()),
        Err(_) => None,
    };
    said.unwrap_or(match response.status() {
        401 => ApiError::SignInRequired,
        403 => ApiError::Forbidden,
        404 => ApiError::NotFound,
        _ => ApiError::Internal,
    })
}

/// Wait `milliseconds`.
async fn wait(window: &web_sys::Window, milliseconds: i32) {
    let waited = js_sys::Promise::new(&mut |done, _| {
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&done, milliseconds);
    });
    let _ = JsFuture::from(waited).await;
}
