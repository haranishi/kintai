//! Bounded MCP stdio transport over a startup-scoped, immutable report snapshot.

mod tools;

use std::io::{self, BufRead, Read, Write};

use serde_json::{Value, json};

use crate::args::{Args, Format};
use crate::failure::Failure;
use crate::report::{self, ReportArgs};
use crate::strict_json::StrictValue;
use crate::text::CommandOutput;

const MAX_LINE: u64 = 65_536;
const PROTOCOL: &str = "2025-11-25";

#[derive(Default, PartialEq, Eq)]
enum State {
    #[default]
    New,
    Initializing,
    Ready,
}

/// Parse the startup scope. Tools can only narrow this scope, never widen it.
///
/// # Errors
/// Invalid CLI arguments or an explicit output format.
pub fn parse(mut raw: Args) -> Result<ReportArgs, Failure> {
    if raw.text("--format")?.is_some() {
        return Err(Failure::invalid(
            "mcp の出力は JSON-RPC です（--format は使いません）",
        ));
    }
    let mut args = ReportArgs::parse(raw)?;
    args.format = Format::Json;
    Ok(args)
}

fn error(id: Value, code: i32, message: &str) -> Value {
    let mut response = json!({"jsonrpc": "2.0", "error": {"code": code, "message": message}});
    response["id"] = id;
    response
}

fn success(id: Value, result: Value) -> Value {
    let mut response = json!({"jsonrpc": "2.0"});
    response["id"] = id;
    response["result"] = result;
    response
}

fn valid_id(id: &Value) -> bool {
    id.is_string() || id.is_i64() || id.is_u64()
}

fn empty_params(params: Option<&Value>) -> bool {
    params.is_none_or(|params| {
        params
            .as_object()
            .is_some_and(|params| params.keys().all(|key| key == "_meta"))
    })
}

struct Session {
    state: State,
    report: Value,
}

impl Session {
    fn handle(&mut self, message: &Value) -> Option<Value> {
        let id = message.get("id").cloned().unwrap_or(Value::Null);
        let usable_id = if valid_id(&id) {
            id.clone()
        } else {
            Value::Null
        };
        let Some(object) = message.as_object() else {
            return Some(error(Value::Null, -32600, "Invalid Request"));
        };
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return Some(error(usable_id, -32600, "Invalid Request"));
        };
        if message["jsonrpc"] != "2.0" || (object.contains_key("id") && !valid_id(&id)) {
            return Some(error(usable_id, -32600, "Invalid Request"));
        }
        let params = message.get("params");
        if !object.contains_key("id") {
            // AC-15.1: notifications never produce a reply, including unknown notifications.
            if method == "notifications/initialized"
                && self.state == State::Initializing
                && empty_params(params)
            {
                self.state = State::Ready;
            }
            return None;
        }
        if method == "ping" {
            return Some(if empty_params(params) {
                success(id, json!({}))
            } else {
                error(id, -32602, "Invalid params")
            });
        }
        if method == "initialize" {
            return Some(self.initialize(id, params));
        }
        if self.state != State::Ready {
            return Some(error(
                id,
                -32002,
                "Complete initialization before using tools",
            ));
        }
        Some(match method {
            "tools/list" if empty_params(params) => success(id, tools::list()),
            "tools/list" => error(id, -32602, "Invalid params"),
            "tools/call" => match tools::call(&self.report, params) {
                Ok(result) => success(id, result),
                Err(()) => error(id, -32602, "Invalid tool call"),
            },
            _ => error(id, -32601, "Method not found"),
        })
    }

    fn initialize(&mut self, id: Value, params: Option<&Value>) -> Value {
        if self.state != State::New {
            return error(id, -32600, "Session is already initialized");
        }
        let Some(params) = params.and_then(Value::as_object) else {
            return error(id, -32602, "Invalid initialization params");
        };
        let version = params.get("protocolVersion").and_then(Value::as_str);
        let client = params.get("clientInfo");
        if version.is_none()
            || !params.get("capabilities").is_some_and(Value::is_object)
            || !client
                .is_some_and(|client| client["name"].is_string() && client["version"].is_string())
        {
            return error(id, -32602, "Invalid initialization params");
        }
        self.state = State::Initializing;
        let version = if version == Some("2025-06-18") {
            "2025-06-18"
        } else {
            PROTOCOL
        };
        success(
            id,
            json!({
                "protocolVersion": version,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "kintai", "title": "kintai attendance reference reports", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "Read-only snapshot of the CLI-selected employees and month. Results are unreviewed reference calculations. Refused records must never be treated as zero hours or an approved payroll. No payroll amounts or accounting journals are produced."
            }),
        )
    }
}

fn write_message(writer: &mut impl Write, message: &Value) -> io::Result<()> {
    serde_json::to_writer(&mut *writer, message)?;
    writer.write_all(b"\n")?;
    writer.flush()
}

fn serve(mut reader: impl BufRead, mut writer: impl Write, report: Value) -> io::Result<()> {
    let mut session = Session {
        state: State::New,
        report,
    };
    loop {
        let mut line = Vec::new();
        let count = (&mut reader)
            .take(MAX_LINE + 1)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            return Ok(());
        }
        // AC-15.4: allocation is bounded even without a newline. Close on oversized input.
        if u64::try_from(line.len()).unwrap_or(u64::MAX) > MAX_LINE {
            write_message(
                &mut writer,
                &error(
                    Value::Null,
                    -32600,
                    "Request exceeds 64 KiB; session closed",
                ),
            )?;
            return Ok(());
        }
        let response = match serde_json::from_slice::<StrictValue>(&line) {
            Ok(StrictValue(value)) => session.handle(&value),
            Err(_) => Some(error(Value::Null, -32700, "Parse error")),
        };
        if let Some(response) = response {
            write_message(&mut writer, &response)?;
        }
    }
}

/// Read one report, then serve MCP until stdin closes. AC-15.1–AC-15.5.
///
/// # Errors
/// Invalid input files or transport failures. No raw input is included in errors.
pub fn run(args: &ReportArgs) -> Result<CommandOutput, Failure> {
    let snapshot = report::run(args)?;
    let report = serde_json::from_str(&snapshot.stdout)
        .map_err(|_| Failure::invalid("MCP の集計結果を作成できません"))?;
    serve(io::stdin().lock(), io::stdout().lock(), report)
        .map_err(|_| Failure::invalid("MCP の入出力を続行できません"))?;
    Ok(CommandOutput {
        stdout: String::new(),
        exit: 0,
    })
}
