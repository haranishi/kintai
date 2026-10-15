//! 日本の労働基準法に基づく時間の区分、36協定の上限、年次有給休暇の規則。根拠つきの日付表を持つ。
//!
//! この版（Phase 0 の第1周）は、通常の労働時間制の時間の区分だけを持つ。
//! ファイルも通信も扱わない純粋な計算で、依存は `kintai-core` だけ。
//!
//! - [`provenance`] は、法令の根拠（法令名、条項、e-Gov の URL と版ID、確認日）。
//! - [`policy`] は、方針と、その検査（範囲外は不正な入力、対応外は [`UnsupportedCode`]）。
//! - [`shift`] は、勤務と休憩と、その検査。
//! - [`classify`](mod@classify) は、時間の区分（法定内・時間外・法定休日・60時間超・深夜）。
//! - [`limits36`] は、36協定の上限の判定（第2周）。
//! - [`leave`] は、年次有給休暇の付与・失効・残数・5日取得義務・管理簿（第2周）。
//! - [`holidays`] は、国民の祝日（第2周）。
//! - [`breaks`] は、休憩の不足の警告（第2周）。
//! - [`report`] は、月次の集計と保存期限の目安（第2周）。
//! - [`explain`] は、区間ごとの区分と割増の下限の説明（第2周）。
//!
//! 入力の検査は `docs/spec/validation.md` の順に、[`WorkPolicy::validate`] →
//! [`ShiftSet::new`] → [`classify()`] と呼ぶ。
//!
//! ```
//! use kintai_core::Weekday;
//! use kintai_jp_labor::{ShiftInput, ShiftSet, WorkPolicy, WorkSystem, classify};
//!
//! let policy = WorkPolicy {
//!     work_system: WorkSystem::Standard,
//!     scheduled_minutes_per_day: 420,
//!     week_start: None,
//!     weekly_limit_minutes: None,
//!     special_44h_confirmed: None,
//!     legal_holiday_weekday: Some(Weekday::Sunday),
//!     month_start_day: 1,
//!     ambiguity: None,
//!     worker_kind: None,
//!     holiday_system: None,
//!     holiday_swaps: 0,
//!     night_window: None,
//! }
//! .validate()
//! .unwrap();
//! let shift = ShiftInput {
//!     start: "2026-10-07T09:00".parse().unwrap(),
//!     end: "2026-10-08T05:00".parse().unwrap(),
//!     breaks: vec![("2026-10-07T12:00".parse().unwrap(), "2026-10-07T13:00".parse().unwrap())],
//! };
//! let shifts = ShiftSet::new(&[shift]).unwrap();
//! let result = classify(&policy, &shifts, None).unwrap();
//! let day = &result.days[0];
//! assert_eq!((day.total, day.within, day.extra_within, day.overtime), (1140, 480, 60, 660));
//! assert_eq!((day.night, day.night_overtime), (420, 420));
//! ```

pub mod breaks;
pub mod classify;
pub mod explain;
pub mod holidays;
pub mod leave;
pub mod limits36;
pub mod policy;
pub mod provenance;
pub mod report;
pub mod shift;

pub use classify::{
    Ambiguity, AmbiguityCode, Classification, ClassifyError, DayResult, PeriodResult, RecordDate,
    Refusal, RefusalCode, classify, find_ambiguities,
};
pub use policy::{
    AmbiguityMode, HolidaySystem, NightWindow, PolicyError, UnsupportedCode, ValidPolicy,
    WorkPolicy, WorkSystem, WorkerKind,
};
pub use provenance::{Law, Provenance};
pub use shift::{Shift, ShiftError, ShiftInput, ShiftSet};
