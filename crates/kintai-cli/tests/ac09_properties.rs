//! 無作為の入力でバイナリを動かす性質の試験。
//!
//! - AC-09.2: 出力の合計の関係（total、night）が常に成り立つ。
//! - AC-09.4: どんな入力でも、終了コードは 0・2・3・4 のどれか（パニックやシグナルで終わらない）。
//!   2・3 のときは標準エラーが `{"error": {"code", "message"}}` の JSON で、標準出力は空。

mod common;

use common::{Run, run_case, run_case_bytes, valid_case};
use proptest::prelude::*;
use serde_json::{Value, json};

const WEEKDAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

fn minutes_to_text(minutes: i64) -> String {
    // 2026-07-01T00:00 からの分を、表記にする（試験の中だけの簡単な換算）。
    let day = minutes.div_euclid(1440);
    let of_day = minutes.rem_euclid(1440);
    let lengths = [31, 31, 30, 31, 30, 31];
    let mut month = 7;
    let mut rest = day;
    for length in lengths {
        if rest < length {
            break;
        }
        rest -= length;
        month += 1;
    }
    format!(
        "2026-{month:02}-{:02}T{:02}:{:02}",
        rest + 1,
        of_day / 60,
        of_day % 60
    )
}

/// 2026年7月から12月の中に、重ならない勤務を並べたケース（方針も無作為）。
fn random_case() -> impl Strategy<Value = Value> {
    let shift = (
        0_i64..2000,
        2_i64..1440,
        prop::option::of((1_i64..200, 1_i64..90)),
    );
    (
        prop::sample::select(WEEKDAYS.to_vec()),
        prop::sample::select(WEEKDAYS.to_vec()),
        1_i64..=28,
        0_i64..=480,
        any::<bool>(),
        any::<bool>(),
        prop::collection::vec(shift, 0..30),
    )
        .prop_map(
            |(week_start, holiday, month_start, scheduled, special, use_default, raw)| {
                let mut cursor = 0;
                let mut shifts = Vec::new();
                for (gap, len, rest) in raw {
                    let start = cursor + gap;
                    let end = start + len;
                    let mut item = json!({
                        "start": minutes_to_text(start),
                        "end": minutes_to_text(end),
                    });
                    if let Some((offset, rest_len)) = rest {
                        let rest_start = start + offset;
                        let rest_end = rest_start + rest_len;
                        if rest_end < end {
                            item["breaks"] =
                                json!([[minutes_to_text(rest_start), minutes_to_text(rest_end)]]);
                        }
                    }
                    shifts.push(item);
                    cursor = end;
                }
                json!({
                    "schema": "kintai.case.v1",
                    "policy": {
                        "work_system": "standard",
                        "scheduled_minutes_per_day": scheduled,
                        "week_start": week_start,
                        "weekly_limit_minutes": if special { 2640 } else { 2400 },
                        "special_44h_confirmed": special,
                        "legal_holiday_weekday": holiday,
                        "month_start_day": month_start,
                        "ambiguity": if use_default { "use_default" } else { "reject" },
                    },
                    "shifts": shifts,
                })
            },
        )
}

fn n(value: &Value, field: &str) -> i64 {
    value[field].as_i64().unwrap()
}

/// 終了コードと出力の形が、決まりのとおりか。
fn check_shape(run: &Run) {
    match run.exit {
        0 | 4 => {
            assert_eq!(run.stderr, "");
            assert!(run.stdout.ends_with('\n') && !run.stdout.trim_end().contains('\n'));
            let out = run.output();
            assert_eq!(out["schema"], "kintai.classify.v1");
            for day in out["days"].as_array().unwrap() {
                assert_eq!(
                    n(day, "total"),
                    n(day, "within") + n(day, "overtime") + n(day, "holiday")
                );
                assert_eq!(
                    n(day, "night"),
                    n(day, "night_within") + n(day, "night_overtime") + n(day, "night_holiday")
                );
                assert!(n(day, "over60") <= n(day, "overtime"));
                assert!(n(day, "night_over60") <= n(day, "night_overtime"));
            }
            for period in out["periods"].as_array().unwrap() {
                assert_eq!(
                    n(period, "total"),
                    n(period, "within") + n(period, "overtime") + n(period, "holiday")
                );
            }
            if run.exit == 4 {
                assert_eq!(out["refused"]["code"], "AmbiguousBoundary");
                assert_eq!(out["days"], json!([]));
            } else {
                assert_eq!(out["refused"], Value::Null);
            }
        }
        2 | 3 => {
            assert_eq!(run.stdout, "");
            let code = run.error_code();
            assert_eq!(run.exit == 2, code == "InvalidInput", "{code}");
        }
        other => panic!("決まりにない終了コード {other}: {run:?}"),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// AC-09.2 無作為の正しいケースで、合計の関係が成り立つ。拒否は終了コード4。
    #[test]
    fn ac09_2_random_cases_keep_the_sums(case in random_case()) {
        let run = run_case(&case);
        prop_assert!(run.exit == 0 || run.exit == 4, "{run:?}");
        check_shape(&run);
    }

    /// AC-09.4 正しいケースの JSON を壊しても、終了コードは決まりのどれかで、パニックしない。
    #[test]
    fn ac09_4_mutated_json_never_crashes(
        edits in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>(), 0_u8..3), 1..6),
    ) {
        let mut bytes = serde_json::to_vec(&valid_case()).unwrap();
        for (index, byte, kind) in edits {
            let at = index.index(bytes.len().max(1)).min(bytes.len());
            match kind {
                0 => { if at < bytes.len() { bytes[at] = byte; } }
                1 => bytes.insert(at, byte),
                _ => { if at < bytes.len() { bytes.remove(at); } }
            }
        }
        check_shape(&run_case_bytes(&bytes));
    }

    /// AC-09.4 任意のバイト列は不正な入力（終了コード2）。
    #[test]
    fn ac09_4_arbitrary_bytes_are_invalid_input(bytes in prop::collection::vec(any::<u8>(), 0..64)) {
        let run = run_case_bytes(&bytes);
        prop_assert_eq!(run.exit, 2, "{:?}", run);
        check_shape(&run);
    }
}
