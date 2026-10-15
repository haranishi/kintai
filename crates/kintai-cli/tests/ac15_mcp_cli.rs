//! Real stdio sessions; no mocked tool dispatcher or model provider.
mod common;

use common::{fixture_path, kintai};
use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};

fn init(version: &str) -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"synthetic-test","version":"1"}}})
}
fn ready() -> Value {
    json!({"jsonrpc":"2.0","method":"notifications/initialized"})
}
fn request(id: i64, method: &str, params: Value) -> Value {
    let mut message = json!({"jsonrpc":"2.0","id":id,"method":method});
    message["params"] = params;
    message
}
fn call(id: i64, name: &str, arguments: Value) -> Value {
    let mut params = json!({"name":name});
    params["arguments"] = arguments;
    request(id, "tools/call", params)
}

fn session_bytes(input: &[u8], extra: &[&str]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_kintai"))
        .args([
            "mcp",
            "--policy",
            &fixture_path("policy.toml"),
            "--punches",
            &fixture_path("punches.csv"),
            "--month",
            "2026-10",
        ])
        .args(extra)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stderr, b"");
    let stdout = String::from_utf8(output.stdout).unwrap();
    stdout
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn session(messages: &[Value], extra: &[&str]) -> Vec<Value> {
    let mut input = messages
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    if !input.is_empty() {
        input.push('\n');
    }
    session_bytes(input.as_bytes(), extra)
}

/// AC-15.1: initialization, negotiated versions, readiness, ping, notifications and EOF.
#[test]
fn ac15_1_lifecycle_and_protocol_negotiation() {
    for version in ["2025-11-25", "2025-06-18", "future-version"] {
        let out = session(
            &[
                request(0, "tools/list", json!({})),
                init(version),
                request(2, "tools/list", json!({})),
                ready(),
                request(3, "ping", json!({})),
                request(4, "tools/list", json!({})),
                json!({"jsonrpc":"2.0","method":"unknown/notification"}),
            ],
            &[],
        );
        assert_eq!(out.len(), 5);
        assert_eq!(out[0]["error"]["code"], -32002);
        assert_eq!(
            out[1]["result"]["protocolVersion"],
            if version == "2025-06-18" {
                version
            } else {
                "2025-11-25"
            }
        );
        assert_eq!(out[2]["error"]["code"], -32002);
        assert_eq!(out[3]["result"], json!({}));
        assert_eq!(out[4]["result"]["tools"].as_array().unwrap().len(), 2);
        for tool in out[4]["result"]["tools"].as_array().unwrap() {
            assert_eq!(tool["annotations"]["readOnlyHint"], true);
            assert_eq!(tool["annotations"]["openWorldHint"], false);
            assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        }
    }
    assert_eq!(session(&[], &[]).len(), 0);
}

/// AC-15.2, AC-15.5: the original report survives transport, including partial refusal.
#[test]
fn ac15_2_report_matches_cli_and_retains_refusal_metadata() {
    let out = session(
        &[
            init("2025-11-25"),
            ready(),
            call(2, "kintai_monthly_report", json!({})),
            call(3, "kintai_employees", json!({})),
        ],
        &[],
    );
    let result = &out[1]["result"];
    assert_eq!(result["isError"], false);
    let structured = &result["structuredContent"];
    assert_eq!(structured["incomplete"], true);
    let original = kintai(&[
        "report",
        "--policy",
        &fixture_path("policy.toml"),
        "--punches",
        &fixture_path("punches.csv"),
        "--month",
        "2026-10",
        "--format",
        "json",
    ]);
    assert_eq!(structured["report"], original.output());
    let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(&text, structured);
    let list = &out[2]["result"]["structuredContent"];
    assert_eq!(list["review_status"], "unreviewed");
    assert_eq!(list["incomplete"], true);
    assert_eq!(list["employees"].as_array().unwrap().len(), 5);
    assert!(list["table_checked_on"].is_string());
}

/// AC-15.3: neither a tool argument nor another employee can expand CLI-selected scope.
#[test]
fn ac15_3_tools_can_only_narrow_the_startup_scope() {
    let out = session(
        &[
            init("2025-11-25"),
            ready(),
            call(2, "kintai_monthly_report", json!({"employee_id":"E001"})),
            call(3, "kintai_monthly_report", json!({"employee_id":"E005"})),
            call(
                4,
                "kintai_monthly_report",
                json!({"path":"PRIVATE_SENTINEL"}),
            ),
            call(5, "kintai_monthly_report", json!({"month":"2026-11"})),
            call(6, "kintai_employees", json!({})),
        ],
        &["--employee", "E001"],
    );
    assert_eq!(
        out[1]["result"]["structuredContent"]["report"]["employees"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(out[1]["result"]["structuredContent"]["incomplete"], false);
    for item in &out[2..5] {
        assert_eq!(item["result"]["isError"], true);
    }
    let all = serde_json::to_string(&out).unwrap();
    assert!(!all.contains("PRIVATE_SENTINEL"));
    assert!(!all.contains("E005"));
    assert_eq!(
        out[5]["result"]["structuredContent"]["employees"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

/// AC-15.4: broken input is redacted, and later valid requests continue.
#[test]
fn ac15_4_invalid_json_batches_ids_and_duplicates_are_recoverable() {
    let mut input = b"PRIVATE_SENTINEL\n[]\n{\"jsonrpc\":\"2.0\",\"id\":null,\"method\":\"ping\"}\n{\"jsonrpc\":\"2.0\",\"id\":1,\"id\":2,\"method\":\"ping\"}\n".to_vec();
    input.extend_from_slice(
        format!(
            "{}\n{}\n{}\n",
            init("2025-11-25"),
            ready(),
            request(2, "ping", json!({}))
        )
        .as_bytes(),
    );
    let out = session_bytes(&input, &[]);
    assert_eq!(out.len(), 6);
    assert_eq!(out[0]["error"]["code"], -32700);
    assert_eq!(out[1]["error"]["code"], -32600);
    assert_eq!(out[2]["error"]["code"], -32600);
    assert_eq!(out[3]["error"]["code"], -32700);
    assert_eq!(out[5]["result"], json!({}));
    assert!(
        !serde_json::to_string(&out)
            .unwrap()
            .contains("PRIVATE_SENTINEL")
    );
}

#[test]
fn ac15_4_oversized_unterminated_line_is_bounded_and_closes_the_session() {
    let out = session_bytes(&vec![b'x'; 65_537], &[]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["error"]["code"], -32600);
}

#[test]
fn invalid_calls_do_not_prevent_a_later_success() {
    let mut messages = vec![init("2025-11-25"), ready()];
    for args in [
        Value::Null,
        json!([]),
        json!({"employee_id":null}),
        json!({"employee_id":"../PRIVATE_SENTINEL"}),
        json!({"employee_id":1}),
    ] {
        messages.push(call(2, "kintai_monthly_report", args));
    }
    messages.push(call(3, "unknown", json!({})));
    messages.push(request(4, "unknown", json!({})));
    messages.push(call(5, "kintai_employees", json!({})));
    let out = session(&messages, &[]);
    for item in &out[1..6] {
        assert_eq!(item["result"]["isError"], true);
    }
    assert_eq!(out[6]["error"]["code"], -32602);
    assert_eq!(out[7]["error"]["code"], -32601);
    assert_eq!(out[8]["result"]["isError"], false);
    assert!(
        !serde_json::to_string(&out)
            .unwrap()
            .contains("PRIVATE_SENTINEL")
    );
}

#[test]
fn malformed_startup_inputs_use_only_stderr_and_do_not_start_transport() {
    let out = kintai(&["mcp", "--format", "json"]);
    assert_eq!(out.exit, 2);
    assert_eq!(out.stdout, "");
    assert_eq!(out.error_code(), "InvalidInput");
}
