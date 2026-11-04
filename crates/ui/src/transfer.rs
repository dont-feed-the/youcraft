//! The browser's side of an upload: the chosen file, sent piece by piece at its offset.
//!
//! Each piece is a slice of the `File` (the browser reads it from disk as it sends, so a file of
//! any size never sits in memory), sent with `PATCH` and the offset it starts at. When the server
//! answers with another offset (a piece that arrived but whose answer was lost), sending resumes
//! from the server's offset. Only the browser has files, so this exists in the `hydrate` build.

use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use youcraft_api::ApiError;
use youcraft_api::records::Upload;

/// The file chosen in the `<input type=file>` that `event` comes from.
pub fn chosen_file(event: &web_sys::Event) -> Option<web_sys::File> {
    let input: web_sys::HtmlInputElement = event.target()?.dyn_into().ok()?;
    input.files()?.get(0)
}

/// Ask the person to confirm `question`.
pub fn confirm(question: &str) -> bool {
    web_sys::window()
        .and_then(|w| w.confirm_with_message(question).ok())
        .unwrap_or(false)
}

/// The offset the server answered with.
fn offset_of(response: &web_sys::Response) -> Option<u64> {
    response.headers().get("upload-offset").ok()??.parse().ok()
}

/// Send `file` to `upload`'s address from its offset on; `sent` hears the bytes received so far.
pub async fn send(
    file: &web_sys::File,
    upload: &Upload,
    sent: impl Fn(u64),
) -> Result<(), ApiError> {
    let window = web_sys::window().ok_or(ApiError::Internal)?;
    let size = file.size() as u64;
    let mut offset = upload.received;
    let mut failures = 0;
    while offset < size {
        let end = offset.saturating_add(upload.piece).min(size);
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
        let response: Option<web_sys::Response> = answer.ok().and_then(|r| r.dyn_into().ok());
        match response {
            Some(response) if response.ok() || response.status() == 409 => {
                offset = offset_of(&response).ok_or(ApiError::Internal)?;
                failures = 0;
                sent(offset);
            }
            Some(response) if response.status() == 413 => {
                return Err(ApiError::TooLarge { limit: size });
            }
            Some(response) if response.status() < 500 && response.status() != 408 => {
                return Err(ApiError::Forbidden);
            }
            // the network or the server failed, or the piece came too slowly: try the same
            // piece again, a few times
            _ => {
                failures += 1;
                if failures > 5 {
                    return Err(ApiError::Internal);
                }
            }
        }
    }
    Ok(())
}
