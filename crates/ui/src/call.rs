//! Run a command from a page: in process while the server renders, over HTTP in the browser.
//!
//! The server gives each rendering a [`Dispatch`] bound to the request's caller; the browser posts
//! the same JSON to `/api/v1/<id>` with its cookies. Either way a page sees one function, typed by
//! the command, and an [`ApiError`] when it fails.

use youcraft_api::{ApiError, Command};

/// Run `C` on `input`.
pub fn call<C: Command>(
    input: C::Input,
) -> impl Future<Output = Result<C::Output, ApiError>> + Send + 'static {
    let body = serde_json::to_value(&input);
    let run = transport::<C>(body.map_err(|_| ApiError::Internal));
    async move {
        let value = run.await?;
        serde_json::from_value(value).map_err(|_| ApiError::Internal)
    }
}

#[cfg(feature = "ssr")]
fn transport<C: Command>(
    body: Result<serde_json::Value, ApiError>,
) -> impl Future<Output = Result<serde_json::Value, ApiError>> + Send + 'static {
    use std::sync::Arc;

    use youcraft_api::Dispatch;

    let dispatch = leptos::prelude::use_context::<Arc<dyn Dispatch>>();
    async move {
        let dispatch = dispatch.ok_or(ApiError::Internal)?;
        dispatch.dispatch(C::ID, body?).await
    }
}

#[cfg(all(feature = "hydrate", not(feature = "ssr")))]
fn transport<C: Command>(
    body: Result<serde_json::Value, ApiError>,
) -> impl Future<Output = Result<serde_json::Value, ApiError>> + Send + 'static {
    send_wrapper::SendWrapper::new(async move { post(C::ID, body?).await })
}

#[cfg(all(feature = "hydrate", not(feature = "ssr")))]
async fn post(id: &str, body: serde_json::Value) -> Result<serde_json::Value, ApiError> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window().ok_or(ApiError::Internal)?;
    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    init.set_credentials(web_sys::RequestCredentials::SameOrigin);
    init.set_body(&wasm_bindgen::JsValue::from_str(&body.to_string()));
    let url = youcraft_api::routes::command(id);
    let request =
        web_sys::Request::new_with_str_and_init(&url, &init).map_err(|_| ApiError::Internal)?;
    request
        .headers()
        .set("content-type", "application/json")
        .map_err(|_| ApiError::Internal)?;
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| ApiError::Internal)?;
    let response: web_sys::Response = response.dyn_into().map_err(|_| ApiError::Internal)?;
    let text = JsFuture::from(response.text().map_err(|_| ApiError::Internal)?)
        .await
        .map_err(|_| ApiError::Internal)?;
    let value: serde_json::Value = serde_json::from_str(&text.as_string().unwrap_or_default())
        .map_err(|_| ApiError::Internal)?;
    if response.ok() {
        Ok(value)
    } else {
        Err(serde_json::from_value(value).unwrap_or(ApiError::Internal))
    }
}
