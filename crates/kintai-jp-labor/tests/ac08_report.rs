//! 月次の集計（`docs/spec/report.md`）。AC-08.1〜08.7 の境界の例と、保存期限の目安。
//! データは合成（従業員IDは E001 など）と、公的な数値例（`vectors/official-examples.json`）。

mod common;

use common::{date, policy, policy_from_json, shift, shift_with_breaks, shifts_from_json, vectors};
use kintai_core::punch::{EmployeeId, PunchError};
use kintai_core::{MonthPeriod, MonthStartDay, Weekday};
use kintai_jp_labor::Provenance;
use kintai_jp_labor::report::{
    INDUSTRIAL_SAFETY_REGULATION, ISR_52NO7NO3_2, LSA_33_1, LSA_38_1, LSA_108, LSA_109,
    LSA_SUPPL_143_1, LedgerRow, MonthlyReport, Notice, REG_24NO7, REG_54_1, REG_54_2,
    REG_56_1_ITEM2, REG_56_1_ITEM5, REG_56_2, REG_56_3, REG_SUPPL_71, ReportError, ReportInput,
    RetentionKind, ReviewStatus, RoundedHours, Rounding, TABLE_CHECKED_ON, ledger, ledger_for,
    retention, round_to_hours,
};
use kintai_jp_labor::{
    AmbiguityCode, AmbiguityMode, Classification, ClassifyError, ShiftInput, ShiftSet, ValidPolicy,
    WorkPolicy, classify,
};

fn valid(policy: &WorkPolicy) -> ValidPolicy {
    policy.validate().unwrap()
}

fn classified(policy: &ValidPolicy, inputs: &[ShiftInput]) -> (ShiftSet, Classification) {
    let shifts = ShiftSet::new(inputs).unwrap();
    let result = classify(policy, &shifts, None).unwrap();
    (shifts, result)
}

fn october() -> MonthPeriod {
    date("2026-10-01").month_period(MonthStartDay::FIRST)
}

/// 項目を（労働日数, 労働時間, 延長, 60時間超, 休日, 深夜, 法定内, 所定外法定内）の組にする。
fn fields(row: &LedgerRow) -> [i64; 8] {
    [
        row.work_days,
        row.work_minutes,
        row.overtime_minutes,
        row.over60_minutes,
        row.holiday_minutes,
        row.night_minutes,
        row.within_minutes,
        row.extra_within_minutes,
    ]
}

/// 2026年10月の合成の週: 水曜の時間外、木曜の夜勤（深夜）、土曜（法定外休日）、日曜（法定休日）。所定420分。
fn sample_week() -> Vec<ShiftInput> {
    vec![
        shift_with_breaks(
            "2026-10-07T09:00",
            "2026-10-07T19:00",
            &[("2026-10-07T12:00", "2026-10-07T13:00")],
        ),
        shift("2026-10-08T22:00", "2026-10-09T06:00"),
        shift("2026-10-10T09:00", "2026-10-10T15:00"),
        shift_with_breaks(
            "2026-10-11T09:00",
            "2026-10-11T20:00",
            &[("2026-10-11T12:00", "2026-10-11T13:00")],
        ),
    ]
}

fn sample_policy() -> ValidPolicy {
    let mut base = policy(Weekday::Sunday);
    base.scheduled_minutes_per_day = 420;
    valid(&base)
}

// ---- AC-08.1 台帳の項目 ----

/// AC-08.1: 台帳の項目が区分の結果から出る。労働時間＝法定内＋時間外＋休日で、深夜は足さない。
#[test]
fn ac08_1_ledger_fields_from_classification() {
    let policy = sample_policy();
    let (_, result) = classified(&policy, &sample_week());
    let row = ledger_for(&policy, &result, october(), Rounding::None).unwrap();
    // 水 540（法定内480・時間外60・所定外法定内60）、木 480（法定内、深夜420、所定外法定内60）、
    // 土 360（法定内）、日 600（法定休日）。
    assert_eq!(fields(&row), [4, 1980, 60, 0, 600, 420, 1320, 120]);
    assert_eq!(
        row.work_minutes,
        row.within_minutes + row.overtime_minutes + row.holiday_minutes
    );
    assert!(row.night_minutes <= row.work_minutes);
    assert_eq!(row.rounded, None);
    assert_eq!(row.period.to_string(), "2026-10-01");
    // 区分の結果の期間の合計とも一致する。
    let period = &result.periods[0];
    assert_eq!(
        (
            period.total,
            period.within,
            period.overtime,
            period.over60,
            period.holiday,
            period.night
        ),
        (
            row.work_minutes,
            row.within_minutes,
            row.overtime_minutes,
            row.over60_minutes,
            row.holiday_minutes,
            row.night_minutes
        )
    );
    // 実労働のある期間をすべて出す形も同じ。
    assert_eq!(ledger(&policy, &result, Rounding::None).unwrap(), vec![row]);
}

/// AC-08.1: 公的な数値例（月60時間超の例）で、延長4380分・60時間超780分・休日120分。
#[test]
fn ac08_1_official_sixty_hour_month() {
    let vector = vectors()["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "mhlw-60h-leaflet-31day-month")
        .unwrap()
        .clone();
    let case = &vector["case"];
    let policy = valid(&policy_from_json(&case["policy"]));
    let shifts = ShiftSet::new(&shifts_from_json(&case["shifts"])).unwrap();
    let data_from = case["data_from"].as_str().map(date);
    let result = classify(&policy, &shifts, data_from).unwrap();
    let rows = ledger(&policy, &result, Rounding::Monthly30Min).unwrap();
    assert_eq!(rows.len(), 1);
    let row = rows[0];
    assert_eq!(row.period.to_string(), "2026-07-01");
    assert_eq!(row.work_days, i64::try_from(shifts.len()).unwrap());
    assert_eq!(
        (
            row.overtime_minutes,
            row.over60_minutes,
            row.holiday_minutes
        ),
        (4380, 780, 120)
    );
    assert_eq!(
        row.rounded,
        Some(RoundedHours {
            overtime: 73,
            holiday: 2,
            night: 0
        })
    );
}

/// AC-08.1: 日をまたぐ勤務は、労働日（始業の日）の月の期間で数える（`use_default` のとき）。
#[test]
fn ac08_1_labor_day_decides_the_period() {
    let mut base = policy(Weekday::Sunday);
    base.ambiguity = Some(AmbiguityMode::UseDefault);
    let policy = valid(&base);
    let (_, result) = classified(&policy, &[shift("2026-09-30T22:00", "2026-10-01T06:00")]);
    let rows = ledger(&policy, &result, Rounding::None).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].period.to_string(), "2026-09-01");
    assert_eq!(fields(&rows[0]), [1, 480, 0, 0, 0, 420, 480, 0]);
    let october_row = ledger_for(&policy, &result, october(), Rounding::None).unwrap();
    assert_eq!(fields(&october_row), [0; 8]);
}

// ---- AC-08.2 法定休日 ----

/// AC-08.2: 法定休日の労働は、8時間を超えても延長時間数に入らない（休日労働時間数に入る）。
/// 法定外休日（土曜）の8時間超は延長時間数に入る。
#[test]
fn ac08_2_overtime_excludes_legal_holiday_work() {
    let policy = valid(&policy(Weekday::Sunday));
    let (_, sunday) = classified(&policy, &[shift("2026-10-11T07:00", "2026-10-11T19:00")]);
    let row = ledger_for(&policy, &sunday, october(), Rounding::None).unwrap();
    assert_eq!(
        (
            row.work_minutes,
            row.holiday_minutes,
            row.overtime_minutes,
            row.over60_minutes
        ),
        (720, 720, 0, 0)
    );
    let (_, saturday) = classified(&policy, &[shift("2026-10-10T07:00", "2026-10-10T19:00")]);
    let row = ledger_for(&policy, &saturday, october(), Rounding::None).unwrap();
    assert_eq!(
        (
            row.work_minutes,
            row.holiday_minutes,
            row.overtime_minutes,
            row.within_minutes
        ),
        (720, 0, 240, 480)
    );
}

// ---- AC-08.3 30分の丸め ----

/// AC-08.3: 30分未満は切り捨て、30分以上は切り上げ（月の合計に対して）。
#[test]
fn ac08_3_round_to_hours_at_29_and_30() {
    for (minutes, hours) in [
        (0, 0),
        (29, 0),
        (30, 1),
        (59, 1),
        (60, 1),
        (89, 1),
        (90, 2),
        (4349, 72),
        (4350, 73),
        (4380, 73),
    ] {
        assert_eq!(round_to_hours(minutes), hours, "{minutes}");
    }
}

/// AC-08.3: `monthly_30min` は、延長・休日・深夜の月の合計を丸めた時間を別に出し、元の分と60時間超は変えない。
#[test]
fn ac08_3_monthly_rounding_keeps_minutes_and_over60() {
    let policy = valid(&policy(Weekday::Sunday));
    for (end, overtime, hours) in [("2026-10-07T17:29", 29, 0), ("2026-10-07T17:30", 30, 1)] {
        let (_, result) = classified(&policy, &[shift("2026-10-07T09:00", end)]);
        let rounded = ledger_for(&policy, &result, october(), Rounding::Monthly30Min).unwrap();
        let plain = ledger_for(&policy, &result, october(), Rounding::None).unwrap();
        assert_eq!(rounded.overtime_minutes, overtime);
        assert_eq!(rounded.rounded.map(|r| r.overtime), Some(hours));
        assert_eq!(plain.rounded, None);
        assert_eq!(fields(&rounded), fields(&plain), "元の分は変えない");
    }
    // 60時間超の分は丸めない（丸めた時間に60時間超の欄はなく、分はそのまま）。
    let vector = vectors()["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "mhlw-60h-leaflet-31day-month")
        .unwrap()
        .clone();
    let case = &vector["case"];
    let sixty = valid(&policy_from_json(&case["policy"]));
    let shifts = ShiftSet::new(&shifts_from_json(&case["shifts"])).unwrap();
    let result = classify(&sixty, &shifts, case["data_from"].as_str().map(date)).unwrap();
    let row = ledger(&sixty, &result, Rounding::Monthly30Min).unwrap()[0];
    assert_eq!(row.over60_minutes, 780);
    let RoundedHours {
        overtime,
        holiday,
        night,
    } = row.rounded.unwrap();
    assert_eq!((overtime, holiday, night), (73, 2, 0));
}

// ---- AC-08.4 日ごとの丸めはない ----

/// AC-08.4: `rounding` の値は `none` と `monthly_30min` だけ。日ごとの切り捨てなどを選ぶ値はない。
#[test]
fn ac08_4_no_daily_rounding_value() {
    assert_eq!(Rounding::default(), Rounding::None);
    assert_eq!(Rounding::ALL.map(Rounding::code), ["none", "monthly_30min"]);
    for rounding in Rounding::ALL {
        assert_eq!(Rounding::from_code(rounding.code()), Some(rounding));
        // 値が増えたら、この試験の書き直しが要る（日ごとの丸めを足さないための見張り）。
        match rounding {
            Rounding::None | Rounding::Monthly30Min => {}
        }
    }
    for other in [
        "daily",
        "daily_floor",
        "daily_15min",
        "floor",
        "15min",
        "monthly_15min",
        "monthly_30min_daily",
        "None",
        "MONTHLY_30MIN",
        " none",
        "",
    ] {
        assert_eq!(Rounding::from_code(other), None, "{other}");
    }
}

// ---- AC-08.5 保存期限 ----

fn dates(kind: RetentionKind, completed: &str, wage_due: &str) -> [String; 4] {
    let r = retention(kind, date(completed), date(wage_due)).unwrap();
    [
        r.start.to_string(),
        r.statutory_current.to_string(),
        r.statutory_base.to_string(),
        r.product_default.to_string(),
    ]
}

/// AC-08.5: 賃金台帳と重要な書類は、完了の日と支払期日の「遅い方」から数える。
#[test]
fn ac08_5_later_of_completion_and_wage_due() {
    for kind in [RetentionKind::WageLedger, RetentionKind::Attendance] {
        // 支払期日が遅い（厚生労働省の例: タイムカード完結4/30、支払期日5/15 → 起算5/15）。
        assert_eq!(
            dates(kind, "2026-04-30", "2026-05-15"),
            ["2026-05-15", "2029-05-15", "2031-05-15", "2031-05-15"]
        );
        // 完了の日が遅い。
        assert_eq!(
            dates(kind, "2026-06-01", "2026-05-15"),
            ["2026-06-01", "2029-06-01", "2031-06-01", "2031-06-01"]
        );
        // 同じ日。
        assert_eq!(dates(kind, "2026-05-15", "2026-05-15")[0], "2026-05-15");
    }
    // 年次有給休暇管理簿は、期間の満了の日と支払期日の遅い方。本則5年、当分の間3年。
    assert_eq!(
        dates(RetentionKind::LeaveRegister, "2027-09-30", "2027-10-15"),
        ["2027-10-15", "2030-10-15", "2032-10-15", "2032-10-15"]
    );
}

/// AC-08.5: 労働時間の状況の記録は、法定の起算日がないので、完了の日から数える（製品の決めごと）。本則も3年。
#[test]
fn ac08_5_status_record_counts_from_completion() {
    let r = retention(
        RetentionKind::StatusRecord,
        date("2026-04-30"),
        date("2026-05-15"),
    )
    .unwrap();
    assert!(!r.start_is_statutory);
    assert_eq!(
        dates(RetentionKind::StatusRecord, "2026-04-30", "2026-05-15"),
        ["2026-04-30", "2029-04-30", "2029-04-30", "2031-04-30"]
    );
    for kind in [
        RetentionKind::WageLedger,
        RetentionKind::Attendance,
        RetentionKind::LeaveRegister,
    ] {
        assert!(
            retention(kind, date("2026-04-30"), date("2026-05-15"))
                .unwrap()
                .start_is_statutory
        );
    }
}

/// AC-08.5: 期限は民法の数え方（初日を入れず、応当日の前日に満了。応当日がなければ月末）。
#[test]
fn ac08_5_civil_code_period_edges() {
    // 2月29日から: 翌日（3月1日）から数え、3年後の3月1日の前日。
    assert_eq!(
        dates(RetentionKind::WageLedger, "2028-02-29", "2028-02-29"),
        ["2028-02-29", "2031-02-28", "2033-02-28", "2033-02-28"]
    );
    // うるう年の2月28日から: 翌日（2月29日）から数え、応当日のない年は2月の末日。
    assert_eq!(
        dates(RetentionKind::WageLedger, "2028-02-28", "2028-02-28"),
        ["2028-02-28", "2031-02-28", "2033-02-28", "2033-02-28"]
    );
    // 平年の2月28日から: 翌日（3月1日）から数え、5年後の3月1日の前日が、うるう年の2月29日になる。
    assert_eq!(
        dates(RetentionKind::WageLedger, "2027-02-28", "2027-02-28"),
        ["2027-02-28", "2030-02-28", "2032-02-29", "2032-02-29"]
    );
    // 年末と入力の範囲の最後の日。
    assert_eq!(
        dates(RetentionKind::Attendance, "2026-12-31", "2026-12-31"),
        ["2026-12-31", "2029-12-31", "2031-12-31", "2031-12-31"]
    );
    assert_eq!(
        dates(RetentionKind::Attendance, "2099-12-31", "2099-12-31"),
        ["2099-12-31", "2102-12-31", "2104-12-31", "2104-12-31"]
    );
}

/// AC-08.5・08.6: 種類の値、年数、根拠の条文、出力の表示。
#[test]
fn ac08_5_retention_kinds() {
    assert_eq!(
        RetentionKind::ALL.map(RetentionKind::code),
        [
            "wage-ledger",
            "attendance",
            "status-record",
            "leave-register"
        ]
    );
    for kind in RetentionKind::ALL {
        assert_eq!(RetentionKind::from_code(kind.code()), Some(kind));
        assert_eq!(kind.statutory_current_years(), 3);
        assert_eq!(kind.product_default_years(), 5);
        assert_ne!(kind.provenances(), &[]);
        let r = retention(kind, date("2026-04-30"), date("2026-05-15")).unwrap();
        assert_eq!(r.kind, kind);
        assert_eq!(r.table_checked_on, "2026-10-15");
        assert_eq!(r.review_status.code(), "unreviewed");
    }
    assert_eq!(RetentionKind::StatusRecord.statutory_base_years(), 3);
    assert_eq!(RetentionKind::WageLedger.statutory_base_years(), 5);
    for other in ["wage_ledger", "timecard", "", "Attendance"] {
        assert_eq!(RetentionKind::from_code(other), None, "{other}");
    }
    let citations: Vec<String> = RetentionKind::WageLedger
        .provenances()
        .iter()
        .map(Provenance::citation)
        .collect();
    assert_eq!(
        citations,
        [
            "労働基準法第109条",
            "労働基準法附則第143条第1項",
            "労働基準法施行規則第56条第1項第2号",
            "労働基準法施行規則第56条第2項",
        ]
    );
}

// ---- AC-08.6 表の確認日と確認の状態 ----

/// AC-08.6: 月次の集計の出力には、`table_checked_on` と `review_status: "unreviewed"` と4つの注意が必ず付く
/// （従業員がいなくても、拒否された従業員だけでも）。
#[test]
fn ac08_6_meta_is_always_present() {
    let sample = sample_policy();
    let empty = MonthlyReport::build(&sample, Rounding::None, Some(october()), &[]).unwrap();
    assert_eq!(empty.table_checked_on, "2026-10-15");
    assert_eq!(TABLE_CHECKED_ON, kintai_jp_labor::provenance::CONFIRMED_ON);
    assert_eq!(empty.review_status, ReviewStatus::Unreviewed);
    assert_eq!(empty.review_status.code(), "unreviewed");
    assert_eq!(
        empty.notices.map(Notice::code),
        [
            "NoHolidaySwap",
            "NoCrossEmployerAggregation",
            "NoEmergencyWork",
            "WeeklyAssignmentIsProductRule"
        ]
    );
    for notice in Notice::ALL {
        assert_ne!(notice.message(), "");
    }
    assert_eq!(
        Notice::NoCrossEmployerAggregation
            .provenance()
            .map(|p| p.citation()),
        Some("労働基準法第38条第1項".to_string())
    );
    assert_eq!(
        Notice::NoEmergencyWork.provenance().map(|p| p.citation()),
        Some("労働基準法第33条第1項".to_string())
    );
    assert_eq!(Notice::NoHolidaySwap.provenance(), None);

    // 曖昧さで拒否された従業員は、台帳の項目を空にして refused を付ける。表示はやはり付く。
    let reject = valid(&policy(Weekday::Sunday));
    let inputs = [shift("2026-10-10T22:00", "2026-10-11T02:00")];
    let (shifts, result) = classified(&reject, &inputs);
    let id = EmployeeId::new("E001").unwrap();
    let report = MonthlyReport::build(
        &reject,
        Rounding::Monthly30Min,
        Some(october()),
        &[ReportInput {
            employee: &id,
            shifts: &shifts,
            classification: &result,
        }],
    )
    .unwrap();
    assert_eq!(report.table_checked_on, "2026-10-15");
    assert_eq!(report.review_status, ReviewStatus::Unreviewed);
    assert_eq!(report.rounding, Rounding::Monthly30Min);
    let entry = &report.employees[0];
    assert!(entry.refused.is_some());
    assert_eq!(entry.ledger, Vec::new());
    assert_eq!(
        ledger_for(&reject, &result, october(), Rounding::None),
        Err(ReportError::Refused)
    );
    assert_eq!(
        ledger(&reject, &result, Rounding::None),
        Err(ReportError::Refused)
    );
}

/// 月次の集計: 従業員ID順、同じ従業員の2回は誤り、期間の行は実労働がなくても1行、
/// 休憩の不足と曖昧さの記録はその期間のものだけ。
#[test]
fn monthly_report_entries() {
    let mut base = policy(Weekday::Sunday);
    base.ambiguity = Some(AmbiguityMode::UseDefault);
    let policy = valid(&base);
    let first = [
        shift("2026-09-30T09:00", "2026-09-30T18:00"),
        shift("2026-10-07T09:00", "2026-10-07T18:00"),
        shift("2026-10-31T22:00", "2026-11-01T02:00"),
    ];
    let (first_shifts, first_result) = classified(&policy, &first);
    let (second_shifts, second_result) = classified(&policy, &[]);
    let (e1, e2) = (
        EmployeeId::new("E2").unwrap(),
        EmployeeId::new("E10").unwrap(),
    );
    let inputs = [
        ReportInput {
            employee: &e1,
            shifts: &first_shifts,
            classification: &first_result,
        },
        ReportInput {
            employee: &e2,
            shifts: &second_shifts,
            classification: &second_result,
        },
    ];
    let report = MonthlyReport::build(&policy, Rounding::None, Some(october()), &inputs).unwrap();
    let ids: Vec<&str> = report
        .employees
        .iter()
        .map(|e| e.employee.as_str())
        .collect();
    assert_eq!(ids, ["E10", "E2"]);
    // E10 は勤務がなくても、10月の行が1つ（すべて0）。
    assert_eq!(report.employees[0].ledger.len(), 1);
    assert_eq!(fields(&report.employees[0].ledger[0]), [0; 8]);
    // E2 の10月: 10/7 と 10/31 の夜勤（労働日は10/31）。9/30 の勤務は数えない。
    let e2_row = report.employees[1].ledger[0];
    assert_eq!(e2_row.work_days, 2);
    assert_eq!(e2_row.work_minutes, 540 + 240);
    // 休憩の不足は10月の労働日だけ（9/30 は出さない）。
    let shortage_days: Vec<String> = report.employees[1]
        .break_shortages
        .iter()
        .map(|s| s.date.to_string())
        .collect();
    assert_eq!(shortage_days, ["2026-10-07"]);
    // 曖昧さの記録（10/31 の夜勤は、法定休日・週・月の境界をまたぐ）。
    let codes: Vec<AmbiguityCode> = report.employees[1]
        .ambiguities
        .iter()
        .map(|a| a.code)
        .collect();
    assert_eq!(
        codes,
        [
            AmbiguityCode::HolidayBoundary,
            AmbiguityCode::WeekBoundary,
            AmbiguityCode::MonthBoundary
        ]
    );
    // 期間を指定しなければ、実労働のある期間をすべて出す（9月と10月）。
    let all = MonthlyReport::build(&policy, Rounding::None, None, &inputs).unwrap();
    let periods: Vec<String> = all.employees[1]
        .ledger
        .iter()
        .map(|row| row.period.to_string())
        .collect();
    assert_eq!(periods, ["2026-09-01", "2026-10-01"]);
    assert_eq!(all.employees[1].break_shortages.len(), 2);
    // 同じ従業員を2回渡すと誤り。
    let twice = [inputs[0], inputs[0]];
    assert_eq!(
        MonthlyReport::build(&policy, Rounding::None, None, &twice),
        Err(ReportError::DuplicateEmployee)
    );
    // 起算日の違う月の期間は誤り。
    let sixteenth = date("2026-10-20").month_period(MonthStartDay::new(16).unwrap());
    assert_eq!(
        ledger_for(&policy, &first_result, sixteenth, Rounding::None),
        Err(ReportError::PeriodMismatch)
    );
}

/// 誤りの文と、区分の計算の誤りからの変換。
#[test]
fn report_error_messages() {
    for error in [
        ReportError::Refused,
        ReportError::PeriodMismatch,
        ReportError::DuplicateEmployee,
        ReportError::Overflow,
        ReportError::OutOfRange,
    ] {
        assert_ne!(error.to_string(), "", "{error:?}");
    }
    assert_eq!(
        ReportError::from(ClassifyError::Overflow),
        ReportError::Overflow
    );
}

// ---- AC-08.7 従業員ID ----

/// AC-08.7: 月次の集計の従業員は [`EmployeeId`] で、`[A-Za-z0-9][A-Za-z0-9._-]{0,63}` に限る
/// （CSV に書き出すときに、式として実行される先頭の文字を持てない）。
#[test]
fn ac08_7_report_uses_checked_employee_ids() {
    for bad in ["=1+1", "+1", "-1", "@A", "", "E 1"] {
        assert_eq!(
            EmployeeId::new(bad),
            Err(PunchError::InvalidEmployeeId),
            "{bad}"
        );
    }
    assert_eq!(EmployeeId::new("E001").unwrap().as_str(), "E001");
}

// ---- 根拠の条文 ----

fn statutes() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/legal/sources/statutes-2026-10-15.txt"
    ))
    .unwrap()
}

/// 法令の部分（`法令ID=…` の見出しから、次の法令の見出しまで）。同じ法令が2か所（本則と附則）にあれば、つなぐ。
fn law_section(statutes: &str, law_id: &str) -> String {
    let marker = format!("法令ID={law_id}");
    statutes
        .match_indices(&marker)
        .map(|(start, _)| {
            let rest = &statutes[start..];
            let end = rest[marker.len()..]
                .find("法令ID=")
                .map_or(rest.len(), |i| i + marker.len());
            &rest[..end]
        })
        .collect()
}

/// 根拠の照合の1行: 定数、その法令の法令ID、その法令の部分に含まれるはずの文言。
type StatuteCase = (Provenance, &'static str, &'static [&'static str]);

const ACT: &str = "322AC0000000049";
const REGULATION: &str = "322M40000100023";
const SAFETY: &str = "347M50002000032";

const STATUTE_CASES: [StatuteCase; 14] = [
    (
        LSA_33_1,
        ACT,
        &[
            "第三十三条 （災害等による臨時の必要がある場合の時間外労働等）",
            "災害その他避けることのできない事由によつて、臨時の必要がある場合",
        ],
    ),
    (
        LSA_38_1,
        ACT,
        &[
            "第三十八条 （時間計算）",
            "労働時間は、事業場を異にする場合においても、労働時間に関する規定の適用については通算する。",
        ],
    ),
    (LSA_108, ACT, &["第百八条 （賃金台帳）", "賃金台帳を調製し"]),
    (
        LSA_109,
        ACT,
        &[
            "第百九条 （記録の保存）",
            "[1項] 使用者は、労働者名簿、賃金台帳及び雇入れ、解雇、災害補償、賃金その他労働関係に関する重要な書類を五年間保存しなければならない。",
        ],
    ),
    (
        LSA_SUPPL_143_1,
        ACT,
        &[
            "【附則】第百四十三条",
            "第百九条の規定の適用については、当分の間、同条中「五年間」とあるのは、「三年間」とする。",
        ],
    ),
    (
        REG_54_1,
        REGULATION,
        &[
            "第五十四条",
            "四 労働日数",
            "五 労働時間数",
            "その延長時間数、休日労働時間数及び深夜労働時間数",
        ],
    ),
    (
        REG_54_2,
        REGULATION,
        &["その就業規則に基いて算定する労働時間数を以てこれに代えることができる。"],
    ),
    (
        REG_56_1_ITEM2,
        REGULATION,
        &["第五十六条", "二 賃金台帳については、最後の記入をした日"],
    ),
    (
        REG_56_1_ITEM5,
        REGULATION,
        &["五 賃金その他労働関係に関する重要な書類については、その完結の日"],
    ),
    (
        REG_56_2,
        REGULATION,
        &[
            "当該記録に係る賃金の支払期日が同項第二号又は第五号に掲げる日より遅い場合には、当該支払期日を起算日とする。",
        ],
    ),
    (
        REG_56_3,
        REGULATION,
        &["[3項] 前項の規定は、", "年次有給休暇管理簿"],
    ),
    (
        REG_24NO7,
        REGULATION,
        &[
            "第二十四条の七",
            "当該有給休暇を与えた期間中及び当該期間の満了後五年間保存しなければならない。",
        ],
    ),
    (
        REG_SUPPL_71,
        REGULATION,
        &[
            "【附則】第七十一条",
            "第二十四条の七",
            "「五年間」とあるのは、「三年間」とする。",
        ],
    ),
    (
        ISR_52NO7NO3_2,
        SAFETY,
        &[
            "第五十二条の七の三",
            "[2項] 事業者は、前項に規定する方法により把握した労働時間の状況の記録を作成し、三年間保存するための必要な措置を講じなければならない。",
        ],
    ),
];

/// 月次の集計と保存期限の根拠の定数が、取得した条文の記録（版・条項・文言）に合う。
#[test]
fn provenance_matches_the_retrieved_statutes() {
    let statutes = statutes();
    let revisions = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/legal/sources/revisions-2026-10-15.txt"
    ))
    .unwrap();
    let law = INDUSTRIAL_SAFETY_REGULATION;
    assert!(revisions.contains(law.revision_id));
    assert!(revisions.contains("施行日 2026-10-01"));
    assert!(statutes.contains(&format!("版ID={}", law.revision_id)));
    assert!(statutes.contains(&format!("出典: {}", law.url)));
    assert!(law.revision_id.starts_with(law.law_id));
    for (provenance, law_id, phrases) in STATUTE_CASES {
        assert_eq!(provenance.confirmed_on, "2026-10-15");
        assert_eq!(provenance.law.law_id, law_id, "{}", provenance.citation());
        let section = law_section(&statutes, law_id);
        for phrase in phrases {
            assert!(
                section.contains(phrase),
                "{}: {phrase}",
                provenance.citation()
            );
        }
    }
    assert_eq!(
        ISR_52NO7NO3_2.citation(),
        "労働安全衛生規則第52条の7の3第2項"
    );
}
