//! Tools expose only a narrowed copy of the startup snapshot, never file access.

use serde_json::{Value, json};

const REPORT_TOOL: &str = "kintai_monthly_report";
const EMPLOYEES_TOOL: &str = "kintai_employees";

pub(super) fn list() -> Value {
    let annotations = json!({"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false});
    json!({"tools": [
        {"name": REPORT_TOOL, "title": "Monthly attendance reference report",
         "description": "Read the configured monthly snapshot, optionally narrowed to one employee. Includes original metadata and refused records. Reference values only; not a payroll or compliance verdict.",
         "inputSchema": {"type": "object", "properties": {"employee_id": {"type": "string", "pattern": "^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$", "description": "Optional employee within the startup scope"}}, "additionalProperties": false},
         "annotations": annotations},
        {"name": EMPLOYEES_TOOL, "title": "Employees in this attendance snapshot",
         "description": "List only the configured employees and whether each monthly calculation was refused or needs review.",
         "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false},
         "annotations": annotations}
    ]})
}

fn result(structured: Value, is_error: bool) -> Value {
    let mut response =
        json!({"content": [{"type": "text", "text": structured.to_string()}], "isError": is_error});
    response["structuredContent"] = structured;
    response
}

fn tool_error() -> Value {
    result(
        json!({"error": {"code": "InvalidInput", "message": "Invalid arguments or employee outside the configured scope"}}),
        true,
    )
}

fn incomplete(report: &Value) -> bool {
    report["employees"].as_array().is_some_and(|employees| {
        employees
            .iter()
            .any(|employee| !employee["refused"].is_null())
    })
}

pub(super) fn call(report: &Value, params: Option<&Value>) -> Result<Value, ()> {
    let params = params.and_then(Value::as_object).ok_or(())?;
    if params
        .keys()
        .any(|key| !matches!(key.as_str(), "name" | "arguments" | "_meta"))
    {
        return Err(());
    }
    let name = params.get("name").and_then(Value::as_str).ok_or(())?;
    if !matches!(name, REPORT_TOOL | EMPLOYEES_TOOL) {
        return Err(());
    }
    let empty = json!({});
    let Some(arguments) = params.get("arguments").unwrap_or(&empty).as_object() else {
        return Ok(tool_error());
    };
    if name == EMPLOYEES_TOOL {
        if !arguments.is_empty() {
            return Ok(tool_error());
        }
        let employees: Vec<Value> = report["employees"].as_array().ok_or(())?.iter().map(|employee| {
            let refused = !employee["refused"].is_null();
            let needs_review = refused
                || employee["ambiguities"].as_array().is_some_and(|v| !v.is_empty())
                || employee["break_shortages"].as_array().is_some_and(|v| !v.is_empty());
            json!({"employee_id": employee["employee_id"], "refused": refused, "needs_review": needs_review})
        }).collect();
        return Ok(result(
            json!({"schema": "kintai.employees.v1", "month": report["month"], "employees": employees,
            "table_checked_on": report["table_checked_on"], "review_status": report["review_status"], "incomplete": incomplete(report)}),
            false,
        ));
    }
    if arguments.keys().any(|key| key != "employee_id") {
        return Ok(tool_error());
    }
    let mut scoped = report.clone();
    if let Some(employee) = arguments.get("employee_id") {
        let Some(employee) = employee
            .as_str()
            .filter(|id| crate::args::employee(id, "employee_id").is_ok())
        else {
            return Ok(tool_error());
        };
        let found = report["employees"]
            .as_array()
            .ok_or(())?
            .iter()
            .find(|item| item["employee_id"] == employee);
        let Some(found) = found else {
            return Ok(tool_error());
        };
        scoped["employees"] = json!([found]);
    }
    Ok(result(
        json!({"incomplete": incomplete(&scoped), "report": scoped}),
        false,
    ))
}
