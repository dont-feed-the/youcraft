//! The Model Context Protocol at `/mcp`: agents run the site's commands as tools.
//!
//! It speaks MCP's Streamable HTTP transport without streams: each JSON-RPC request posted is
//! answered with one JSON response, and a notification with 202; it keeps no session. Every tool
//! is a command of [`descriptors`], named by its id with `-` for `.` (agents' tool names take no
//! dot, and ids hold no `-`, so a name maps back to exactly one id), and runs through the same
//! dispatch as `POST /api/v1/<id>`, as the owner of the personal token, who sees only the tools
//! their role allows. A command that fails is a tool result marked `isError`, holding the error's
//! JSON; a malformed message or an unknown tool or method is a JSON-RPC error.
//!
//! Only a personal token signs an agent in, never a browser's cookie, so no other site's page can
//! act through it; a browser's request from another origin is refused all the same, as the
//! protocol asks against DNS rebinding.

use std::sync::LazyLock;

use axum::Json;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Map, Value, json};
use youcraft_api::command::{Descriptor, descriptors};

use crate::dispatch;
use crate::site::{Caller, Credential, Site};

/// The protocol revisions it speaks, the newest first.
const REVISIONS: [&str; 3] = [NEWEST, "2025-03-26", "2024-11-05"];
/// The revision it offers a client that asks for one it does not speak.
const NEWEST: &str = "2025-06-18";
/// The header in which a client names the revision after initializing.
const REVISION_HEADER: &str = "mcp-protocol-version";
/// What it tells an agent on initializing.
const INSTRUCTIONS: &str = "Each tool is a command of YouCraft, a video site, run as the owner of \
                            the personal token. Videos, channels and playlists are named by \
                            11-character ids; a list that continues gives `next` to ask for more.";

/// JSON-RPC's error codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// Every command, described once.
static COMMANDS: LazyLock<Vec<Descriptor>> = LazyLock::new(descriptors);

/// A JSON-RPC error: its code and message.
type Failure = (i64, String);

/// Answer the message `body` that `caller` posted with `headers`.
pub async fn answer(site: &Site, caller: Caller, headers: &HeaderMap, body: &[u8]) -> Response {
    let text = |name| headers.get(name).and_then(|value| value.to_str().ok());
    if text(header::ORIGIN.as_str()).is_some_and(|origin| origin != site.config().origin) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if caller.credential != Credential::Token {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
        )
            .into_response();
    }
    if let Some(revision) = text(REVISION_HEADER)
        && !REVISIONS.contains(&revision)
    {
        let message = format!("protocol revision {revision} is not one of {REVISIONS:?}");
        return failure(
            StatusCode::BAD_REQUEST,
            Value::Null,
            (INVALID_REQUEST, message),
        );
    }
    let message: Value = match serde_json::from_slice(body) {
        Ok(message) => message,
        Err(error) => {
            let failed = (PARSE_ERROR, error.to_string());
            return failure(StatusCode::BAD_REQUEST, Value::Null, failed);
        }
    };
    let Some(message) = message
        .as_object()
        .filter(|m| m.get("jsonrpc") == Some(&json!("2.0")))
    else {
        let failed = (INVALID_REQUEST, "one JSON-RPC 2.0 message".to_owned());
        return failure(StatusCode::BAD_REQUEST, Value::Null, failed);
    };
    let (Some(method), Some(id)) = (
        message.get("method").and_then(Value::as_str),
        message.get("id"),
    ) else {
        // A notification, or the answer to a request: the server sends none that needs one.
        return StatusCode::ACCEPTED.into_response();
    };
    let params = message.get("params").unwrap_or(&Value::Null);
    let result = match method {
        "initialize" => Ok(initialize(params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools(caller) })),
        "tools/call" => call(site, caller, params).await,
        _ => Err((METHOD_NOT_FOUND, format!("{method}: no such method"))),
    };
    match result {
        Ok(result) => Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })).into_response(),
        Err(failed) => failure(StatusCode::OK, id.clone(), failed),
    }
}

/// The JSON-RPC error `failed` in answer to the request `id`, with `status`.
fn failure(status: StatusCode, id: Value, (code, message): Failure) -> Response {
    let error = json!({ "code": code, "message": message });
    (
        status,
        Json(json!({ "jsonrpc": "2.0", "id": id, "error": error })),
    )
        .into_response()
}

/// The answer to `initialize`: the revision the client asked for if it is spoken, else the
/// newest, and what the server offers.
fn initialize(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let revision = REVISIONS
        .into_iter()
        .find(|revision| Some(*revision) == asked)
        .unwrap_or(NEWEST);
    json!({
        "protocolVersion": revision,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": {
            "name": "youcraft",
            "title": "YouCraft",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "instructions": INSTRUCTIONS,
    })
}

/// The name of the tool that runs the command `id`.
pub fn tool_name(id: &str) -> String {
    id.replace('.', "-")
}

/// Whether `schema` describes JSON objects, which alone MCP takes as a tool's output schema.
fn describes_objects(schema: &Value) -> bool {
    schema.get("type").and_then(Value::as_str) == Some("object")
}

/// The tools `caller` may run.
fn tools(caller: Caller) -> Vec<Value> {
    COMMANDS
        .iter()
        .filter(|command| caller.role.allows(command.requires))
        .map(|command| {
            let mut tool = Map::new();
            tool.insert("name".to_owned(), tool_name(&command.id).into());
            tool.insert("title".to_owned(), command.id.clone().into());
            tool.insert("description".to_owned(), command.summary.clone().into());
            tool.insert("inputSchema".to_owned(), command.input.clone());
            if describes_objects(&command.output) {
                tool.insert("outputSchema".to_owned(), command.output.clone());
            }
            Value::Object(tool)
        })
        .collect()
}

/// The answer to `tools/call`: the command's output, or its error, as a tool result.
async fn call(site: &Site, caller: Caller, params: &Value) -> Result<Value, Failure> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| (INVALID_PARAMS, "params.name: the tool".to_owned()))?;
    let command = COMMANDS
        .iter()
        .find(|command| tool_name(&command.id) == name)
        .ok_or_else(|| (INVALID_PARAMS, format!("{name}: no such tool")))?;
    let input = params
        .get("arguments")
        .filter(|arguments| !arguments.is_null())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let mut result = Map::new();
    match dispatch::run(site, caller, &command.id, input).await {
        Ok(output) => {
            result.insert("content".to_owned(), text(&output));
            if describes_objects(&command.output) {
                result.insert("structuredContent".to_owned(), output);
            }
            result.insert("isError".to_owned(), false.into());
        }
        Err(error) => {
            let error = serde_json::to_value(error).unwrap_or(Value::Null);
            result.insert("content".to_owned(), text(&error));
            result.insert("isError".to_owned(), true.into());
        }
    }
    Ok(Value::Object(result))
}

/// `value` as the text content of a tool result.
fn text(value: &Value) -> Value {
    json!([{ "type": "text", "text": value.to_string() }])
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::http::Request;
    use youcraft_api::{Grant, Role};

    use super::*;
    use crate::http::tests::{ORIGIN, command_request, send, sign_in, site};

    /// A personal token of a new account signed in as `sub`, with `role`: a viewer's unless an
    /// invitation made it more.
    async fn token(site: &Site, routes: &Router, sub: &str, role: Grant) -> String {
        if role != Grant::Viewer {
            site.store()
                .invite(&format!("{sub}@example.com"), role, 1)
                .unwrap();
        }
        let cookie = sign_in(routes, sub).await;
        let made = command_request("token.create", r#"{"name": "agent"}"#, &cookie);
        let (_, _, body) = send(routes, made).await;
        let made: Value = serde_json::from_str(&body).unwrap();
        made["secret"].as_str().unwrap().to_owned()
    }

    /// `message` posted to `/mcp` with `token`; the status, the headers and the JSON answer.
    async fn post(routes: &Router, message: &Value, token: &str) -> (StatusCode, HeaderMap, Value) {
        let request = Request::post(youcraft_api::routes::MCP)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json, text/event-stream")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::from(message.to_string()))
            .unwrap();
        let (status, headers, body) = send(routes, request).await;
        (
            status,
            headers,
            serde_json::from_str(&body).unwrap_or_default(),
        )
    }

    fn request(method: &str, params: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": 7, "method": method, "params": params })
    }

    #[tokio::test]
    async fn only_a_personal_token_opens_it() {
        let (_, routes) = site();
        let initialize = request("initialize", json!({ "protocolVersion": NEWEST }));
        let (status, headers, _) = post(&routes, &initialize, "yct_wrong").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(headers[header::WWW_AUTHENTICATE], "Bearer");
        let cookie = sign_in(&routes, "ada").await;
        let with_cookie = Request::post(youcraft_api::routes::MCP)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ORIGIN, ORIGIN)
            .header(header::COOKIE, cookie)
            .body(Body::from(initialize.to_string()))
            .unwrap();
        assert_eq!(send(&routes, with_cookie).await.0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn an_agent_initializes_and_pings() {
        let (site, routes) = site();
        let token = token(&site, &routes, "ada", Grant::Viewer).await;
        let asked = request("initialize", json!({ "protocolVersion": "2025-03-26" }));
        let (status, _, answer) = post(&routes, &asked, &token).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(answer["id"], 7);
        assert_eq!(answer["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(answer["result"]["serverInfo"]["name"], "youcraft");
        let unknown = request("initialize", json!({ "protocolVersion": "1999-01-01" }));
        let (_, _, answer) = post(&routes, &unknown, &token).await;
        assert_eq!(answer["result"]["protocolVersion"], NEWEST);
        let initialized = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert_eq!(
            post(&routes, &initialized, &token).await.0,
            StatusCode::ACCEPTED
        );
        let (_, _, answer) = post(&routes, &request("ping", Value::Null), &token).await;
        assert_eq!(answer["result"], json!({}));
        let (_, _, answer) = post(&routes, &request("resources/list", json!({})), &token).await;
        assert_eq!(answer["error"]["code"], METHOD_NOT_FOUND);
        let (status, _, answer) = post(&routes, &json!([asked]), &token).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(answer["error"]["code"], INVALID_REQUEST);
    }

    #[tokio::test]
    async fn the_tools_are_the_commands_the_owner_may_run() {
        let (site, routes) = site();
        for (sub, role) in [("ada", Grant::Viewer), ("bob", Grant::Admin)] {
            let token = token(&site, &routes, sub, role).await;
            let (_, _, answer) = post(&routes, &request("tools/list", json!({})), &token).await;
            let tools = answer["result"]["tools"].as_array().unwrap();
            let expected: Vec<String> = descriptors()
                .iter()
                .filter(|command| Role::from(role).allows(command.requires))
                .map(|command| tool_name(&command.id))
                .collect();
            let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
            assert_eq!(names, expected);
            for tool in tools {
                let name = tool["name"].as_str().unwrap();
                assert!(
                    name.len() <= 64
                        && name
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
                    "{name}: agents take [A-Za-z0-9_-]{{1,64}}"
                );
                assert_eq!(tool["inputSchema"]["type"], "object", "{name}");
            }
        }
    }

    #[tokio::test]
    async fn a_tool_runs_its_command_as_the_owner() {
        let (site, routes) = site();
        let token = token(&site, &routes, "ada", Grant::Viewer).await;
        let me = request("tools/call", json!({ "name": "account-me" }));
        let (_, _, answer) = post(&routes, &me, &token).await;
        let result = &answer["result"];
        assert_eq!(result["isError"], false, "{answer}");
        let text: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text["channel"]["handle"], "adalovelace");

        let feed = request(
            "tools/call",
            json!({ "name": "feed-home", "arguments": {} }),
        );
        let (_, _, answer) = post(&routes, &feed, &token).await;
        assert_eq!(
            answer["result"]["structuredContent"]["videos"],
            json!([]),
            "{answer}"
        );

        let promote = json!({ "handle": "@adalovelace", "role": "admin" });
        let forbidden = request(
            "tools/call",
            json!({ "name": "admin-set_role", "arguments": promote }),
        );
        let (_, _, answer) = post(&routes, &forbidden, &token).await;
        assert_eq!(answer["result"]["isError"], true);
        let error: Value =
            serde_json::from_str(answer["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(error["error"], "forbidden");

        let unknown = request("tools/call", json!({ "name": "video.get" }));
        let (_, _, answer) = post(&routes, &unknown, &token).await;
        assert_eq!(answer["error"]["code"], INVALID_PARAMS);
    }
}
