// Copyright (c) Michael Grier

//! `munin-binlog-mcp` — MCP (Model Context Protocol) server that exposes
//! MSBuild binary log (`.binlog`) files to AI agents such as GitHub Copilot.
//!
//! The server speaks JSON-RPC 2.0 over stdio using newline-delimited messages.
//! Each tool invocation operates on an in-memory `BinlogIndex` (from the
//! `munin` crate) loaded from a `.binlog` file. Multiple binlog files can be
//! open simultaneously, keyed by opaque session handles.

use std::io::{self, BufRead};

use munin_binlog_mcp::tools;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── JSON-RPC 2.0 wire types ───────────────────────────────────────────────────

/// An incoming JSON-RPC 2.0 message (request or notification).
#[derive(Deserialize)]
struct Message {
    #[allow(dead_code)]
    jsonrpc: String,
    /// Absent for notifications; present for requests.
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

/// An outgoing JSON-RPC 2.0 response.
#[derive(Serialize)]
struct Response {
    jsonrpc: &'static str,
    id: Value,
    #[serde(flatten)]
    body: ResponseBody,
}

#[derive(Serialize)]
#[serde(untagged)]
enum ResponseBody {
    Ok { result: Value },
    Err { error: RpcError },
}

#[derive(Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

/// JSON-RPC 2.0 reserved error codes.
mod code {
    pub const PARSE_ERROR: i32 = -32700;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
}

// ── event loop ────────────────────────────────────────────────────────────────

fn main() {
    eprintln!(
        "munin-binlog-mcp {ver} starting (pid={pid})",
        ver = env!("CARGO_PKG_VERSION"),
        pid = std::process::id(),
    );

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut sessions = tools::SessionMap::new();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("munin-binlog-mcp: stdin error: {e}");
                break;
            }
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let msg: Message = match serde_json::from_str(trimmed) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("munin-binlog-mcp: parse error: {e}");
                send_error(
                    &mut out,
                    Value::Null,
                    code::PARSE_ERROR,
                    format!("parse error: {e}"),
                );
                continue;
            }
        };

        // Notifications have no id — no response is sent.
        let id = match msg.id {
            None => continue,
            Some(ref v) if v.is_null() => continue,
            Some(v) => v,
        };

        let body = dispatch(msg.method.as_str(), msg.params, &mut sessions);
        eprintln!("munin-binlog-mcp: dispatched '{}'", msg.method);
        send_response(
            &mut out,
            Response {
                jsonrpc: "2.0",
                id,
                body,
            },
        );
    }
}

// ── dispatch ──────────────────────────────────────────────────────────────────

fn dispatch(method: &str, params: Option<Value>, sessions: &mut tools::SessionMap) -> ResponseBody {
    match method {
        "initialize" => ResponseBody::Ok {
            result: serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "munin-binlog-mcp",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        },

        "tools/list" => ResponseBody::Ok {
            result: serde_json::json!({ "tools": tools::list() }),
        },

        "tools/call" => {
            let params = match params {
                Some(p) => p,
                None => {
                    return ResponseBody::Err {
                        error: RpcError {
                            code: code::INVALID_PARAMS,
                            message: "tools/call requires params".into(),
                        },
                    }
                }
            };

            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::json!({}));

            match tools::call(name, &args, sessions) {
                Ok(text) => ResponseBody::Ok {
                    result: serde_json::json!({
                        "content": [{ "type": "text", "text": text }]
                    }),
                },
                Err(e) => {
                    eprintln!("munin-binlog-mcp: tool '{name}' failed: {e}");
                    ResponseBody::Ok {
                        result: serde_json::json!({
                            "content": [{ "type": "text", "text": format!("error: {e}") }],
                            "isError": true
                        }),
                    }
                }
            }
        }

        "ping" => ResponseBody::Ok {
            result: serde_json::json!({}),
        },

        "shutdown" => ResponseBody::Ok {
            result: Value::Null,
        },

        _ => ResponseBody::Err {
            error: RpcError {
                code: code::METHOD_NOT_FOUND,
                message: format!("method not found: {method}"),
            },
        },
    }
}

// ── I/O helpers ───────────────────────────────────────────────────────────────

fn send_response(out: &mut impl io::Write, response: Response) {
    match serde_json::to_string(&response) {
        Ok(mut s) => {
            s.push('\n');
            let _ = out.write_all(s.as_bytes());
            let _ = out.flush();
        }
        Err(e) => eprintln!("munin-binlog-mcp: serialization error: {e}"),
    }
}

fn send_error(out: &mut impl io::Write, id: Value, code: i32, message: String) {
    send_response(
        out,
        Response {
            jsonrpc: "2.0",
            id,
            body: ResponseBody::Err {
                error: RpcError { code, message },
            },
        },
    );
}
