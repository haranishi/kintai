//! 方針の検査（`docs/spec/unsupported.md`、`docs/spec/validation.md`）。
//!
//! - AC-11.1: 対応外の表のコードごとに、該当する指定があると計算を拒否する（終了コード3は `kintai-cli` の試験）。
//! - AC-11.2: 拒否のエラーに、コードと条文を含める。
//! - AC-11.3: `legal_holiday_weekday` のない方針は `LegalHolidayUnspecified` で拒否する。
//! - AC-11.4: 拒否した方針では計算できない（黙って0や通常制として出さない）。
//! - AC-13.1: 検査の順序（範囲 → 対応外）。同じ入力に複数の誤りがあっても最初のものを返す。
//! - AC-13.2: 対応外の指定の表の全行が、対応するコードを返す。表の順に見て最初に当たる。

mod common;

use common::{policy, with_defaults_spelled_out};
use kintai_core::{MonthStartDay, Weekday};
use kintai_jp_labor::{
    AmbiguityMode, HolidaySystem, NightWindow, PolicyError, UnsupportedCode, WorkPolicy,
    WorkSystem, WorkerKind,
};

fn base() -> WorkPolicy {
    policy(Weekday::Sunday)
}

fn unsupported(p: &WorkPolicy) -> UnsupportedCode {
    match p.validate() {
        Err(PolicyError::Unsupported(code)) => code,
        other => panic!("対応外にならない: {other:?}"),
    }
}

/// AC-11.1 AC-13.2 `work_system` の11の値が、対応するコードを返す。
#[test]
fn ac11_1_work_systems() {
    let table = [
        (
            WorkSystem::VariableMonthly,
            UnsupportedCode::VariableMonthly,
        ),
        (WorkSystem::VariableYearly, UnsupportedCode::VariableYearly),
        (
            WorkSystem::VariableWeeklyIrregular,
            UnsupportedCode::VariableWeeklyIrregular,
        ),
        (WorkSystem::Flex, UnsupportedCode::Flex),
        (
            WorkSystem::Discretionary,
            UnsupportedCode::DiscretionaryDeemed,
        ),
        (WorkSystem::OffsiteDeemed, UnsupportedCode::OffsiteDeemed),
        (WorkSystem::Managerial, UnsupportedCode::Managerial),
        (
            WorkSystem::AgricultureExempt,
            UnsupportedCode::AgricultureExempt,
        ),
        (
            WorkSystem::SurveillanceIntermittent,
            UnsupportedCode::SurveillanceIntermittent,
        ),
        (
            WorkSystem::NightDutyPermit,
            UnsupportedCode::NightDutyPermit,
        ),
        (
            WorkSystem::HighlyProfessional,
            UnsupportedCode::HighlyProfessional,
        ),
    ];
    for (system, code) in table {
        let mut p = base();
        p.work_system = system;
        assert_eq!(unsupported(&p), code, "{system:?}");
        assert_eq!(system.unsupported(), Some(code));
    }
    assert_eq!(WorkSystem::Standard.unsupported(), None);
}

/// AC-11.1 AC-13.2 `worker_kind` の7つの値が、対応するコードを返す。
#[test]
fn ac11_1_worker_kinds() {
    let table = [
        (WorkerKind::Minor, UnsupportedCode::Minor),
        (
            WorkerKind::PregnantOrPostpartum,
            UnsupportedCode::PregnantOrPostpartum,
        ),
        (
            WorkerKind::ResearchDevelopment,
            UnsupportedCode::ResearchDevelopment,
        ),
        (WorkerKind::Driver, UnsupportedCode::AutomobileDriver),
        (WorkerKind::Physician, UnsupportedCode::Physician),
        (
            WorkerKind::DisasterRecoveryConstruction,
            UnsupportedCode::DisasterRecoveryConstruction,
        ),
        (WorkerKind::HazardousWork, UnsupportedCode::HazardousWork),
    ];
    for (kind, code) in table {
        let mut p = base();
        p.worker_kind = Some(kind);
        assert_eq!(unsupported(&p), code, "{kind:?}");
        assert_eq!(kind.unsupported(), Some(code));
    }
    assert_eq!(WorkerKind::Standard.unsupported(), None);
}

/// AC-11.1 AC-13.2 `holiday_system`・`holiday_swaps`・`night_window`・`weekly_limit_minutes` の行。
#[test]
fn ac11_1_other_rows() {
    let mut p = base();
    p.holiday_system = Some(HolidaySystem::FourWeeks);
    assert_eq!(unsupported(&p), UnsupportedCode::FourWeekHolidays);
    p.holiday_system = Some(HolidaySystem::ContinuousRotation);
    assert_eq!(unsupported(&p), UnsupportedCode::ContinuousRotation);
    assert_eq!(HolidaySystem::Weekly.unsupported(), None);

    let mut p = base();
    p.holiday_swaps = 1;
    assert_eq!(unsupported(&p), UnsupportedCode::HolidaySwap);
    p.holiday_swaps = 0;
    assert!(p.validate().is_ok(), "空の配列（0件）は対応外でない");

    let mut p = base();
    p.night_window = Some(NightWindow::From23To06);
    assert_eq!(unsupported(&p), UnsupportedCode::NightWindowOverride);
    assert_eq!(NightWindow::From22To05.unsupported(), None);

    let mut p = base();
    p.weekly_limit_minutes = Some(2640);
    assert_eq!(
        unsupported(&p),
        UnsupportedCode::Weekly44WithoutConfirmation
    );
    p.special_44h_confirmed = Some(false);
    assert_eq!(
        unsupported(&p),
        UnsupportedCode::Weekly44WithoutConfirmation
    );
    p.special_44h_confirmed = Some(true);
    assert!(p.validate().is_ok());
    // 2400 のときは、確認の有無は関係ない。
    p.weekly_limit_minutes = Some(2400);
    p.special_44h_confirmed = Some(false);
    assert!(p.validate().is_ok());
}

/// AC-11.3 法定休日の曜日がない方針は `LegalHolidayUnspecified`。
#[test]
fn ac11_3_legal_holiday_unspecified() {
    let mut p = base();
    p.legal_holiday_weekday = None;
    assert_eq!(unsupported(&p), UnsupportedCode::LegalHolidayUnspecified);
}

/// AC-11.2 拒否のエラーに、コードと条文（法令名と条項）を含める。全コード。
#[test]
fn ac11_2_error_has_code_and_article() {
    assert_eq!(UnsupportedCode::ALL.len(), 25);
    for code in UnsupportedCode::ALL {
        let message = PolicyError::Unsupported(code).to_string();
        assert_ne!(code.provenances().len(), 0, "{code:?}");
        for provenance in code.provenances() {
            assert!(
                message.contains(&provenance.citation()),
                "{code:?}: {message}"
            );
        }
        assert!(message.contains(code.work_style()), "{message}");
        assert!(message.contains(code.difference()), "{message}");
        assert!(code.citation().contains('第'), "{code:?}");
        assert_eq!(code.to_string(), message);
    }
    let citations = [
        (UnsupportedCode::Flex, "労働基準法第32条の3"),
        (
            UnsupportedCode::DiscretionaryDeemed,
            "労働基準法第38条の3、労働基準法第38条の4",
        ),
        (
            UnsupportedCode::VariableWeeklyIrregular,
            "労働基準法第32条の5",
        ),
        (UnsupportedCode::AgricultureExempt, "労働基準法第41条第1号"),
        (
            UnsupportedCode::NightDutyPermit,
            "労働基準法第41条第3号、労働基準法施行規則第23条",
        ),
        (UnsupportedCode::AutomobileDriver, "労働基準法附則第140条"),
        (UnsupportedCode::Physician, "労働基準法附則第141条"),
        (
            UnsupportedCode::DisasterRecoveryConstruction,
            "労働基準法附則第139条第1項",
        ),
        (
            UnsupportedCode::HazardousWork,
            "労働基準法第36条第6項第1号、労働基準法施行規則第18条",
        ),
        (
            UnsupportedCode::Weekly44WithoutConfirmation,
            "労働基準法施行規則第25条の2",
        ),
    ];
    for (code, citation) in citations {
        assert_eq!(code.citation(), citation, "{code:?}");
    }
    assert!(
        UnsupportedCode::ContinuousRotation
            .citation()
            .contains("通達")
    );
    assert!(
        UnsupportedCode::HolidaySwap
            .citation()
            .contains("厚生労働省")
    );
    assert!(
        UnsupportedCode::MonthStartDayMismatch
            .citation()
            .contains("0529001")
    );
    assert_eq!(UnsupportedCode::Flex.basis_note(), "");
    assert_eq!(UnsupportedCode::Flex.code(), "Flex");
}

/// AC-11.2 コードの文字列と並びが、仕様の表（`docs/spec/unsupported.md`）と一致する。
#[test]
fn ac11_2_code_strings_match_the_table() {
    let spec = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/spec/unsupported.md"
    ))
    .unwrap();
    let in_spec: Vec<&str> = spec
        .lines()
        .filter_map(|line| line.strip_prefix("| `"))
        .filter_map(|rest| rest.split_once('`').map(|(code, _)| code))
        .collect();
    let ours: Vec<&str> = UnsupportedCode::ALL.iter().map(|c| c.code()).collect();
    assert_eq!(ours, in_spec, "表の順とコードが一致する");
}

/// AC-11.4 拒否した方針からは、検査済みの方針が作れない（計算に進めない）。
#[test]
fn ac11_4_refused_policy_cannot_be_used() {
    let mut p = base();
    p.work_system = WorkSystem::Flex;
    assert!(p.validate().is_err());
}

/// AC-13.1 範囲外の数値（手順2）は、対応外（手順3）より先に見る。
#[test]
fn ac13_1_range_errors_come_before_unsupported() {
    let mut p = base();
    p.work_system = WorkSystem::Flex;
    p.legal_holiday_weekday = None;
    p.holiday_swaps = 3;
    p.month_start_day = 29;
    assert_eq!(
        p.validate(),
        Err(PolicyError::Invalid {
            field: "month_start_day",
            expected: "1〜28"
        })
    );
    p.month_start_day = 1;
    p.scheduled_minutes_per_day = 481;
    assert!(matches!(
        p.validate(),
        Err(PolicyError::Invalid {
            field: "scheduled_minutes_per_day",
            ..
        })
    ));
    p.scheduled_minutes_per_day = 420;
    p.weekly_limit_minutes = Some(2500);
    assert!(matches!(
        p.validate(),
        Err(PolicyError::Invalid {
            field: "weekly_limit_minutes",
            ..
        })
    ));
    p.weekly_limit_minutes = None;
    assert_eq!(unsupported(&p), UnsupportedCode::Flex);
}

/// AC-13.1 数値の範囲の境界（`scheduled_minutes_per_day` 0〜480、`month_start_day` 1〜28、週 2400/2640）。
#[test]
fn ac13_1_numeric_ranges() {
    let check = |scheduled: i64, start_day: i64, weekly: Option<i64>| {
        let mut p = base();
        p.scheduled_minutes_per_day = scheduled;
        p.month_start_day = start_day;
        p.weekly_limit_minutes = weekly;
        p.special_44h_confirmed = Some(true);
        p.validate()
    };
    assert!(check(0, 1, None).is_ok());
    assert!(check(480, 28, Some(2640)).is_ok());
    assert!(check(-1, 1, None).is_err());
    assert!(check(481, 1, None).is_err());
    assert!(check(420, 0, None).is_err());
    assert!(check(420, 29, None).is_err());
    assert!(check(420, -5, None).is_err());
    assert!(check(420, i64::MAX, None).is_err());
    assert!(check(420, 1, Some(2399)).is_err());
    assert!(check(420, 1, Some(0)).is_err());
    let invalid = check(420, 0, None).unwrap_err();
    assert_eq!(
        invalid.to_string(),
        "policy.month_start_day の値が範囲外です（1〜28）"
    );
}

/// AC-13.2 対応外の表は上から順に見る（`work_system` → `worker_kind` → `holiday_system`
/// → `holiday_swaps` → `night_window` → `legal_holiday_weekday` → `weekly_limit_minutes`）。
#[test]
fn ac13_2_table_order() {
    let mut p = base();
    p.work_system = WorkSystem::Managerial;
    p.worker_kind = Some(WorkerKind::Minor);
    p.holiday_system = Some(HolidaySystem::FourWeeks);
    p.holiday_swaps = 2;
    p.night_window = Some(NightWindow::From23To06);
    p.legal_holiday_weekday = None;
    p.weekly_limit_minutes = Some(2640);
    let expected_in_order = [
        UnsupportedCode::Managerial,
        UnsupportedCode::Minor,
        UnsupportedCode::FourWeekHolidays,
        UnsupportedCode::HolidaySwap,
        UnsupportedCode::NightWindowOverride,
        UnsupportedCode::LegalHolidayUnspecified,
        UnsupportedCode::Weekly44WithoutConfirmation,
    ];
    for expected in expected_in_order {
        assert_eq!(unsupported(&p), expected);
        // 当たった行を直して、次の行が出ることを見る。
        match expected {
            UnsupportedCode::Managerial => p.work_system = WorkSystem::Standard,
            UnsupportedCode::Minor => p.worker_kind = Some(WorkerKind::Standard),
            UnsupportedCode::FourWeekHolidays => p.holiday_system = Some(HolidaySystem::Weekly),
            UnsupportedCode::HolidaySwap => p.holiday_swaps = 0,
            UnsupportedCode::NightWindowOverride => {
                p.night_window = Some(NightWindow::From22To05);
            }
            UnsupportedCode::LegalHolidayUnspecified => {
                p.legal_holiday_weekday = Some(Weekday::Saturday);
            }
            _ => p.special_44h_confirmed = Some(true),
        }
    }
    assert!(p.validate().is_ok());
}

/// 既定値（`docs/spec/case-format.md`）: 週は日曜起算、週2400分、reject。明示しても同じ。
#[test]
fn defaults_are_applied() {
    let implicit = base().validate().unwrap();
    assert_eq!(implicit.week_start(), Weekday::Sunday);
    assert_eq!(implicit.weekly_limit_minutes(), 2400);
    assert_eq!(implicit.ambiguity(), AmbiguityMode::Reject);
    assert_eq!(implicit.legal_holiday_weekday(), Weekday::Sunday);
    assert_eq!(implicit.month_start_day(), MonthStartDay::FIRST);
    assert_eq!(implicit.scheduled_minutes_per_day(), 480);
    let explicit = with_defaults_spelled_out(base()).validate().unwrap();
    assert_eq!(implicit, explicit);
}
