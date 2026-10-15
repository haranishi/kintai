//! 出力の形（`docs/spec/cli-output.md`）の試験。
//!
//! - AC-12.9: `report` `leave` `explain` の JSON と `report` の CSV が、文書の項目と並びのとおりに出る。
//!   `serde_json::Value` は項目を名前の順に並べ替えるので、出力の文字列を、書かれた順のまま読む部品
//!   （[`Node`]）で項目名の並びを見る。
//! - AC-12.10: 人が読む出力（`report` `check` `leave` `explain` `retention`）に、計算に使わなかった事情の注意、
//!   表の確認日、確認の状態、末尾の参考値の1行が付く。

mod common;

use std::fmt;

use common::{
    csv_with_employees, fixture_path, fixture_text, kintai, policy_with_employees, run_case,
    temp_dir, write_file,
};
use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::json;

/// 項目の並びを保ったまま読んだ JSON（値は、オブジェクトと配列のほかは見ない）。
#[derive(Debug)]
enum Node {
    Object(Vec<(String, Node)>),
    Array(Vec<Node>),
    Other,
}

struct NodeVisitor;

impl<'de> Visitor<'de> for NodeVisitor {
    type Value = Node;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON の値")
    }

    fn visit_bool<E>(self, _: bool) -> Result<Node, E> {
        Ok(Node::Other)
    }

    fn visit_i64<E>(self, _: i64) -> Result<Node, E> {
        Ok(Node::Other)
    }

    fn visit_u64<E>(self, _: u64) -> Result<Node, E> {
        Ok(Node::Other)
    }

    fn visit_f64<E>(self, _: f64) -> Result<Node, E> {
        Ok(Node::Other)
    }

    fn visit_str<E>(self, _: &str) -> Result<Node, E> {
        Ok(Node::Other)
    }

    fn visit_unit<E>(self) -> Result<Node, E> {
        Ok(Node::Other)
    }

    fn visit_none<E>(self) -> Result<Node, E> {
        Ok(Node::Other)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Node, D::Error> {
        Node::deserialize(deserializer)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Node, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(Node::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Node, A::Error> {
        let mut entries = Vec::new();
        while let Some((key, value)) = map.next_entry::<String, Node>()? {
            entries.push((key, value));
        }
        Ok(Node::Object(entries))
    }
}

impl<'de> Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Node, D::Error> {
        deserializer.deserialize_any(NodeVisitor)
    }
}

impl Node {
    fn parse(text: &str) -> Node {
        serde_json::from_str(text).unwrap_or_else(|e| panic!("JSON でない（{e}）: {text}"))
    }

    /// オブジェクトの項目名（書かれた順）。
    fn keys(&self) -> Vec<&str> {
        match self {
            Node::Object(entries) => entries.iter().map(|(key, _)| key.as_str()).collect(),
            other => panic!("オブジェクトでない: {other:?}"),
        }
    }

    fn get(&self, key: &str) -> &Node {
        match self {
            Node::Object(entries) => {
                &entries
                    .iter()
                    .find(|(name, _)| name == key)
                    .unwrap_or_else(|| panic!("{key} がない"))
                    .1
            }
            other => panic!("オブジェクトでない: {other:?}"),
        }
    }

    fn at(&self, index: usize) -> &Node {
        match self {
            Node::Array(items) => items
                .get(index)
                .unwrap_or_else(|| panic!("{index} 番目がない")),
            other => panic!("配列でない: {other:?}"),
        }
    }

    /// 項目名の並び（`skip` の名前を除く）。
    fn keys_without(&self, skip: &str) -> Vec<&str> {
        self.keys().into_iter().filter(|key| *key != skip).collect()
    }
}

fn report_json(policy: &str, punches: &str) -> String {
    let dir = temp_dir("shape-report");
    let policy = write_file(&dir, "policy.toml", policy);
    let punches = write_file(&dir, "punches.csv", punches);
    let run = kintai(&[
        "report",
        "--policy",
        &policy,
        "--punches",
        &punches,
        "--month",
        "2026-10",
        "--format",
        "json",
    ]);
    assert!(run.exit == 0 || run.exit == 4, "{run:?}");
    run.stdout
}

const LEDGER: [&str; 10] = [
    "period_start",
    "work_days",
    "work_minutes",
    "overtime_minutes",
    "over60_minutes",
    "holiday_minutes",
    "night_minutes",
    "within_minutes",
    "extra_within_minutes",
    "rounded",
];

/// AC-12.9 `report` の JSON（`kintai.report.v1`）の項目と並び。
#[test]
fn ac12_9_report_json_keys_in_order() {
    let out = Node::parse(&report_json(
        &fixture_text("policy.toml"),
        &fixture_text("punches.csv"),
    ));
    assert_eq!(
        out.keys(),
        [
            "schema",
            "month",
            "period_start",
            "period_end",
            "data_from",
            "rounding",
            "employees",
            "notices",
            "table_checked_on",
            "review_status",
        ]
    );
    let employee = out.get("employees").at(0);
    assert_eq!(
        employee.keys(),
        [
            "employee_id",
            "ledger",
            "refused",
            "ambiguities",
            "break_shortages"
        ]
    );
    assert_eq!(employee.get("ledger").at(0).keys(), LEDGER);
    assert_eq!(
        employee.get("ledger").at(0).get("rounded").keys(),
        ["overtime", "holiday", "night"]
    );
    let e003 = out.get("employees").at(2);
    assert_eq!(
        e003.get("break_shortages").at(0).keys(),
        [
            "date",
            "work_minutes",
            "break_minutes",
            "required_minutes",
            "shortage_minutes"
        ]
    );
    let e005 = out.get("employees").at(4);
    assert_eq!(e005.get("refused").keys(), ["code", "dates", "anomalies"]);
    // 打刻の異常の一覧と、曖昧さの記録（use_default）の1件の並び。
    let broken = fixture_text("punches.csv").replace("E002,2026-10-30 07:00,out\n", "");
    let anomaly = Node::parse(&report_json(&fixture_text("policy.toml"), &broken));
    assert_eq!(
        anomaly
            .get("employees")
            .at(1)
            .get("refused")
            .get("anomalies")
            .at(0)
            .keys(),
        ["kind", "date", "line"]
    );
    let use_default = fixture_text("policy.toml")
        .replace("ambiguity = \"reject\"", "ambiguity = \"use_default\"");
    let recorded = Node::parse(&report_json(&use_default, &fixture_text("punches.csv")));
    assert_eq!(
        recorded
            .get("employees")
            .at(4)
            .get("ambiguities")
            .at(0)
            .keys(),
        ["date", "code"]
    );
}

/// AC-12.9 `report` の CSV の列（見出しと、各行の値の数）。
#[test]
fn ac12_9_report_csv_columns_in_order() {
    let policy = fixture_path("policy.toml");
    let punches = fixture_path("punches.csv");
    let run = kintai(&[
        "report",
        "--policy",
        &policy,
        "--punches",
        &punches,
        "--month",
        "2026-10",
        "--format",
        "csv",
    ]);
    let mut lines = run.stdout.lines();
    assert_eq!(
        lines.next().unwrap(),
        "employee_id,period_start,work_days,work_minutes,overtime_minutes,over60_minutes,holiday_minutes,night_minutes,within_minutes,extra_within_minutes,table_checked_on,review_status,notices"
    );
    for line in lines {
        let cells: Vec<&str> = line.split(',').collect();
        assert_eq!(cells.len(), 13, "{line}");
        assert_eq!(
            &cells[10..],
            [
                "2026-10-15",
                "unreviewed",
                "NoHolidaySwap;NoCrossEmployerAggregation;NoEmergencyWork;WeeklyAssignmentIsProductRule"
            ]
        );
        assert!(
            cells[2..10]
                .iter()
                .all(|cell| cell.bytes().all(|b| b.is_ascii_digit())),
            "{line}"
        );
    }
}

/// AC-12.9 `leave` の JSON（`kintai.leavereport.v1`）の項目と並び。1人分は `kintai.leave.v1` と同じ並び。
#[test]
fn ac12_9_leave_json_keys_in_order() {
    let policy = fixture_path("policy.toml");
    let leaves = fixture_path("leaves.csv");
    let run = kintai(&[
        "leave",
        "--policy",
        &policy,
        "--leaves",
        &leaves,
        "--as-of",
        "2026-10-31",
        "--format",
        "json",
    ]);
    assert_eq!(run.exit, 0, "{run:?}");
    let out = Node::parse(&run.stdout);
    assert_eq!(
        out.keys(),
        [
            "schema",
            "as_of",
            "employees",
            "notices",
            "table_checked_on",
            "review_status"
        ]
    );
    let e001 = out.get("employees").at(0);
    assert_eq!(
        e001.keys(),
        [
            "employee_id",
            "lots",
            "balance_halves",
            "five_day",
            "register",
            "notices"
        ]
    );
    let case = run_case(&json!({
        "schema": "kintai.caseleave.v1", "as_of": "2026-10-31",
        "employee": {"hire_date": "2024-04-01", "weekly_scheduled_days": 5,
            "weekly_scheduled_minutes": 2100, "attendance": ["meets", "meets", "meets"]},
        "taken": [{"date": "2025-05-01", "unit": "full"}]
    }));
    let case = Node::parse(&case.stdout);
    assert_eq!(
        e001.keys_without("employee_id"),
        case.keys_without("schema")
    );
    for list in ["lots", "five_day", "register"] {
        assert_eq!(
            e001.get(list).at(0).keys(),
            case.get(list).at(0).keys(),
            "{list}"
        );
    }
    assert_eq!(
        e001.get("register").at(0).get("taken").at(0).keys(),
        case.get("register").at(0).get("taken").at(0).keys()
    );
    assert_eq!(
        e001.get("lots").at(0).keys(),
        [
            "grant_date",
            "granted_halves",
            "expires",
            "used_halves",
            "remaining_halves",
            "expired_halves"
        ]
    );
}

fn explain_json(employee: &str, date: &str) -> Node {
    let policy = fixture_path("policy.toml");
    let punches = fixture_path("punches.csv");
    let run = kintai(&[
        "explain",
        "--policy",
        &policy,
        "--punches",
        &punches,
        "--employee",
        employee,
        "--date",
        date,
        "--format",
        "json",
    ]);
    assert!(run.exit == 0 || run.exit == 4, "{run:?}");
    Node::parse(&run.stdout)
}

/// AC-12.9 `explain` の JSON（`kintai.explain.v1`）の項目と並び。`day` は `kintai.classify.v1` の `days` と同じ並び。
#[test]
fn ac12_9_explain_json_keys_in_order() {
    let out = explain_json("E001", "2026-10-08");
    assert_eq!(
        out.keys(),
        [
            "schema",
            "employee_id",
            "date",
            "weekday",
            "legal_holiday",
            "national_holiday",
            "holiday_table",
            "day",
            "segments",
            "notes",
            "refused",
            "notices",
            "table_checked_on",
            "review_status",
        ]
    );
    assert_eq!(
        out.get("holiday_table").keys(),
        ["status", "notices", "source"]
    );
    let segment = out.get("segments").at(0);
    assert_eq!(
        segment.keys(),
        ["start", "end", "minutes", "category", "night", "premium"]
    );
    assert_eq!(segment.get("premium").keys(), ["percent", "basis"]);
    assert_eq!(
        segment.get("premium").get("basis").at(0).keys(),
        ["law", "article", "revision_id", "url", "confirmed_on"]
    );
    assert_eq!(out.get("notes").at(0).keys(), ["code", "message"]);
    let classify = run_case(&json!({
        "schema": "kintai.case.v1",
        "policy": {"work_system": "standard", "scheduled_minutes_per_day": 420,
            "legal_holiday_weekday": "sun", "month_start_day": 1},
        "shifts": [{"start": "2026-10-08T09:00", "end": "2026-10-08T23:00"}]
    }));
    let classify = Node::parse(&classify.stdout);
    assert_eq!(out.get("day").keys(), classify.get("days").at(0).keys());
    let holiday = explain_json("E002", "2026-10-12");
    assert_eq!(holiday.get("national_holiday").keys(), ["name", "kind"]);
    let refused = explain_json("E005", "2026-10-07");
    assert_eq!(
        refused.get("refused").keys(),
        ["code", "dates", "anomalies"]
    );
}

/// AC-12.10 人が読む出力（5つのコマンド）に、注意・表の確認日・確認の状態と、末尾の参考値の1行が付く。
/// 表の `P` `C` `L` `H` は合成データの方針・打刻・休暇・履歴、`P1` `C1` は E001 だけに絞ったもの。
#[test]
fn ac12_10_every_text_output_has_the_meta() {
    let dir = temp_dir("shape-text");
    let one_policy = write_file(&dir, "policy.toml", policy_with_employees(&["E001"]));
    let one_punches = write_file(
        &dir,
        "punches.csv",
        csv_with_employees(&fixture_text("punches.csv"), &["E001"]),
    );
    let files = [
        ("P", fixture_path("policy.toml")),
        ("C", fixture_path("punches.csv")),
        ("L", fixture_path("leaves.csv")),
        ("H", fixture_path("history.csv")),
        ("P1", one_policy),
        ("C1", one_punches),
    ];
    let commands = [
        "report --policy P --punches C --month 2026-10",
        "check --policy P --punches C --month 2026-10 --history H",
        "check --policy P1 --punches C1 --month 2026-10 --data-from 2026-09-27",
        "leave --policy P --leaves L --as-of 2026-10-31",
        "explain --policy P --punches C --employee E001 --date 2026-10-08",
        "explain --policy P --punches C --employee E005 --date 2026-10-07",
        "retention --kind leave-register --completed 2026-09-30 --wage-due 2026-10-25",
    ];
    for command in commands {
        let words: Vec<&str> = command
            .split_whitespace()
            .map(|word| {
                files
                    .iter()
                    .find(|(name, _)| *name == word)
                    .map_or(word, |(_, path)| path.as_str())
            })
            .collect();
        let run = kintai(&words);
        assert!(run.exit == 0 || run.exit == 4, "{command}: {run:?}");
        let text = &run.stdout;
        assert!(
            text.contains("注意（計算に使わなかった事情。notices）:"),
            "{command}"
        );
        for code in [
            "NoHolidaySwap",
            "NoCrossEmployerAggregation",
            "NoEmergencyWork",
            "WeeklyAssignmentIsProductRule",
        ] {
            assert!(text.contains(&format!("  - {code}: ")), "{command}: {code}");
        }
        assert!(
            text.contains("表の確認日（table_checked_on）: 2026-10-15\n"),
            "{command}"
        );
        assert!(
            text.contains("確認の状態（review_status）: unreviewed\n"),
            "{command}"
        );
        assert_eq!(
            text.lines().last(),
            Some("この出力は参考値です（専門家の確認前。review_status: unreviewed）。"),
            "{command}"
        );
    }
}
