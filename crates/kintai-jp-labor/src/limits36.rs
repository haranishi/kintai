//! 36協定の上限の判定（`docs/spec/limits36.md`。入出力は `docs/spec/case-format-36-leave.md`）。
//!
//! 協定月ごとの法定時間外 O（法定休日労働を含まない）と法定休日労働 H から、労働基準法第36条
//! 第4項・第5項の月と年の時間と、第6項第2号・第3号の上限を満たしているかを判定する。T＝O＋H。
//! 平均は割らず、直近 n か月の T の合計を `4800 × n` と比べる。浮動小数点は使わない。
//!
//! 判定しないもの: 第6項第1号（坑内労働などの1日2時間）、協定の1日の延長時間と休日労働の日数、
//! 協定の有効性、届出、健康確保措置。副業・兼業先や転職前の時間は通算しない。入力された T
//! （`history` を含む）だけを判定し、出力に [`Limits36Notice::OwnEmployerOnly`] を必ず付ける。
//!
//! 検査の順序は `docs/spec/validation.md` の流儀で、[`Agreement::validate`]（協定の範囲）→
//! [`judge`]（月の分の範囲 → 協定月の日付の整合 → 判定）。月の分は、区分の結果から
//! [`MonthHours::from_period`] か [`months_from_periods`] で作れる。
//!
//! ```
//! use kintai_jp_labor::limits36::{
//!     Agreement, AgreementKind, Finding, HistoryMonth, MonthHours, judge,
//! };
//!
//! let agreement = Agreement {
//!     kind: AgreementKind::General,
//!     year_start_month: 4,
//!     monthly_limit_minutes: Some(2700),
//!     annual_limit_minutes: Some(21_600),
//!     special_monthly_total_limit_minutes: None,
//!     special_annual_limit_minutes: None,
//!     special_months_max: None,
//! }
//! .validate()
//! .unwrap();
//! // 時間外44時間＋休日労働56時間＝100時間は、特別条項に当たらなくても違反（T は6000分未満であること）。
//! let months = [MonthHours {
//!     start: "2021-04-01".parse().unwrap(),
//!     overtime: 2640,
//!     holiday: 3360,
//! }];
//! let history: Vec<HistoryMonth> = ["2020-11-01", "2020-12-01", "2021-01-01", "2021-02-01", "2021-03-01"]
//!     .iter()
//!     .map(|start| HistoryMonth { start: start.parse().unwrap(), total: 0 })
//!     .collect();
//! let result = judge(&agreement, &months, &history).unwrap();
//! assert_eq!(result.months[0].total, 6000);
//! assert_eq!(result.months[0].sum(2), Some(6000));
//! assert_eq!(result.months[0].findings, vec![Finding::MonthlyTotalReached100h]);
//! ```

use std::collections::BTreeMap;
use std::fmt;

use kintai_core::{Date, MonthPeriod};

use crate::classify::PeriodResult;
use crate::provenance::{CONFIRMED_ON, LABOR_STANDARDS_ACT, LSA_36_5, Provenance};

/// 限度時間（1か月）: 45時間＝2700分。労働基準法第36条第4項。O がこれを超える月が**発動月**。
pub const MONTHLY_OVERTIME_LIMIT_MINUTES: i64 = 2700;
/// 限度時間（1年）: 360時間＝21600分。労働基準法第36条第4項。
pub const ANNUAL_OVERTIME_LIMIT_MINUTES: i64 = 21_600;
/// 1か月の時間外＋休日労働は100時間「未満」: T がこの分（6000分）以上なら違反。第36条第6項第2号。
pub const MONTHLY_TOTAL_CAP_MINUTES: i64 = 6000;
/// 2〜6か月の平均の上限（80時間＝4800分）。平均は割らず、合計を `4800 × n` と比べる。第36条第6項第3号。
pub const AVERAGE_CAP_MINUTES: i64 = 4800;
/// 特別条項で協定できる1か月の時間外＋休日労働の最大（100時間未満の最大の分）。第36条第5項。
pub const SPECIAL_MONTHLY_TOTAL_MAX_MINUTES: i64 = 5999;
/// 特別条項で協定できる1年の時間外の最大（720時間＝43200分。通常分を含む総量）。第36条第5項。
pub const SPECIAL_ANNUAL_MAX_MINUTES: i64 = 43_200;
/// 特別条項で協定できる、限度時間を超える月数の最大（1年に6か月）。第36条第5項。
pub const SPECIAL_MONTHS_MAX: i64 = 6;
/// 直近 n か月の合計を見る n（2〜6）。
pub const WINDOW_MONTHS: [u32; 5] = [2, 3, 4, 5, 6];
/// 1か月の分の上限（31日分＝44640分）。協定月の O＋H と、履歴の T はこれ以下（固定幅の整数で桁あふれしないため）。
pub const MONTH_MINUTES_MAX: i64 = 44_640;

const fn act(article: &'static str, summary: &'static str) -> Provenance {
    Provenance {
        law: LABOR_STANDARDS_ACT,
        article,
        summary,
        confirmed_on: CONFIRMED_ON,
    }
}

/// 労働基準法第36条第1項: 協定をし、届け出た場合に、その協定で定めるところにより延長・休日労働ができる。
pub const LSA_36_1: Provenance = act(
    "第36条第1項",
    "書面による協定をし、行政官庁に届け出た場合に、その協定で定めるところによって労働時間を延長し、又は休日に労働させることができる",
);
/// 労働基準法第36条第4項: 限度時間は1か月45時間、1年360時間。
pub const LSA_36_4: Provenance = act(
    "第36条第4項",
    "限度時間は、1か月について45時間及び1年について360時間",
);
/// 労働基準法第36条第6項第2号: 1か月の時間外・休日労働は100時間未満。
pub const LSA_36_6_ITEM2: Provenance = act(
    "第36条第6項第2号",
    "1か月について労働時間を延長して労働させ、及び休日において労働させた時間が100時間未満であること",
);
/// 労働基準法第36条第6項第3号: 2〜6か月の各期間の、時間外・休日労働の1か月当たりの平均は80時間以内。
pub const LSA_36_6_ITEM3: Provenance = act(
    "第36条第6項第3号",
    "直前の1〜5か月を加えた各期間の、時間外・休日労働の1か月当たりの平均時間が80時間を超えないこと",
);

/// 協定の種類（`kind`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AgreementKind {
    /// 協定なし（`none`）。時間外も休日労働もさせられない。
    None,
    /// 一般条項だけ（`general`）。
    General,
    /// 特別条項つき（`special`）。
    Special,
}

/// 36協定（検査の前の値）。範囲外の数値も持てる。[`Agreement::validate`] で検査する。
///
/// 限度の項目は分（回数は回）。`None` は入力で省略（または `null`）されたもの。
/// [`AgreementKind::General`] は月・年の限度が必須、[`AgreementKind::Special`] は特別条項の3項目も必須。
/// その種類で使わない項目があっても、範囲だけ検査して使わない。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Agreement {
    /// 協定の種類。
    pub kind: AgreementKind,
    /// 協定年の起算月（1〜12。種類によらず必須）。
    pub year_start_month: i64,
    /// 一般条項の月の限度（0〜2700分）。
    pub monthly_limit_minutes: Option<i64>,
    /// 一般条項の年の限度（0〜21600分）。
    pub annual_limit_minutes: Option<i64>,
    /// 特別条項の月の時間外＋休日労働の上限（0〜5999分）。
    pub special_monthly_total_limit_minutes: Option<i64>,
    /// 特別条項の年の時間外の上限（0〜43200分）。
    pub special_annual_limit_minutes: Option<i64>,
    /// 特別条項で限度時間を超えられる月数（0〜6回）。
    pub special_months_max: Option<i64>,
}

/// 種類ごとに、判定に使う限度。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Terms {
    None,
    General {
        monthly: i64,
        annual: i64,
    },
    Special {
        monthly: i64,
        annual: i64,
        special_monthly_total: i64,
        special_annual: i64,
        special_months: i64,
    },
}

/// 検査済みの36協定。[`Agreement::validate`] だけが作る。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidAgreement {
    year_start_month: u32,
    terms: Terms,
}

impl ValidAgreement {
    /// 協定の種類。
    #[must_use]
    pub const fn kind(&self) -> AgreementKind {
        match self.terms {
            Terms::None => AgreementKind::None,
            Terms::General { .. } => AgreementKind::General,
            Terms::Special { .. } => AgreementKind::Special,
        }
    }

    /// 協定年の起算月（1〜12）。
    #[must_use]
    pub const fn year_start_month(&self) -> u32 {
        self.year_start_month
    }

    /// 一般条項の月の限度（分）。協定なしでは `None`。
    #[must_use]
    pub const fn monthly_limit_minutes(&self) -> Option<i64> {
        match self.terms {
            Terms::None => None,
            Terms::General { monthly, .. } | Terms::Special { monthly, .. } => Some(monthly),
        }
    }

    /// 一般条項の年の限度（分）。協定なしでは `None`。特別条項つきでは判定に使わない。
    #[must_use]
    pub const fn annual_limit_minutes(&self) -> Option<i64> {
        match self.terms {
            Terms::None => None,
            Terms::General { annual, .. } | Terms::Special { annual, .. } => Some(annual),
        }
    }

    /// 特別条項の月の時間外＋休日労働の上限（分）。特別条項つきだけ。
    #[must_use]
    pub const fn special_monthly_total_limit_minutes(&self) -> Option<i64> {
        match self.terms {
            Terms::Special {
                special_monthly_total,
                ..
            } => Some(special_monthly_total),
            _ => None,
        }
    }

    /// 特別条項の年の時間外の上限（分）。特別条項つきだけ。
    #[must_use]
    pub const fn special_annual_limit_minutes(&self) -> Option<i64> {
        match self.terms {
            Terms::Special { special_annual, .. } => Some(special_annual),
            _ => None,
        }
    }

    /// 特別条項で限度時間を超えられる月数。特別条項つきだけ。
    #[must_use]
    pub const fn special_months_max(&self) -> Option<i64> {
        match self.terms {
            Terms::Special { special_months, .. } => Some(special_months),
            _ => None,
        }
    }
}

/// 36協定の判定の誤り。どれも不正な入力（終了コード2）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Limits36Error {
    /// 協定の項目が範囲外（法定の最大を超える、負など）。
    AgreementOutOfRange {
        /// 項目の名前（例: `monthly_limit_minutes`）。
        field: &'static str,
        /// 受け付ける値の説明。
        expected: &'static str,
    },
    /// 協定の種類で必須の限度の項目がない。
    MissingLimit {
        /// 項目の名前。
        field: &'static str,
    },
    /// 分が範囲外（`months[].overtime`・`months[].holiday` が負、`history[].total` が0〜44640の外）。
    MinutesOutOfRange {
        /// 項目の場所（`months` か `history`）。
        list: &'static str,
        /// 何番目か（0から）。
        index: usize,
        /// 項目の名前（`overtime`・`holiday`・`total`）。
        field: &'static str,
    },
    /// 協定月の O＋H が44640分（31日分）を超える。
    MonthTotalTooLarge {
        /// `months` の何番目か（0から）。
        index: usize,
    },
    /// `months[].start` の日が1〜28でない（協定月の初日は1〜28日）。
    MonthStartDay {
        /// 何番目か（0から）。
        index: usize,
    },
    /// `months` が日付順に隙間なく並んでいない（日が違うものを含む）。
    NotConsecutive {
        /// 何番目か（0から）。
        index: usize,
    },
    /// `history[].start` が協定月の初日でない。
    HistoryNotAgreementMonth {
        /// 何番目か（0から）。
        index: usize,
    },
    /// `history[].start` が `months` の最初の月以後。
    HistoryNotBefore {
        /// 何番目か（0から）。
        index: usize,
    },
    /// `history` に同じ月が2回ある。
    DuplicateHistory {
        /// 2回目の何番目か（0から）。
        index: usize,
    },
    /// 協定月の初日が、日付の型の範囲（1900〜2200年）の外になった。
    DateOutOfRange,
    /// 区分の結果の月の期間の起算日が、そろっていない。
    MismatchedStartDay,
    /// 協定月を数えるとき、次の期間が前に進まなかった（暦の計算の不具合。無限に続けず、誤りで止める）。
    NoProgress,
    /// 分の合計が `i64` を超える。
    Overflow,
}

impl fmt::Display for Limits36Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Limits36Error::AgreementOutOfRange { field, expected } => {
                write!(f, "agreement.{field} の値が範囲外です（{expected}）")
            }
            Limits36Error::MissingLimit { field } => {
                write!(f, "agreement.{field} は、この協定の種類では必須です")
            }
            Limits36Error::MinutesOutOfRange { list, index, field } => {
                write!(f, "{list}[{index}].{field} の値が範囲外です（0〜44640分）")
            }
            Limits36Error::MonthTotalTooLarge { index } => write!(
                f,
                "months[{index}] の overtime と holiday の合計が44640分（31日分）を超えています"
            ),
            Limits36Error::MonthStartDay { index } => write!(
                f,
                "months[{index}].start の日は1〜28です（協定月の初日。29〜31日は使えません）"
            ),
            Limits36Error::NotConsecutive { index } => write!(
                f,
                "months[{index}].start が、前の協定月の次の協定月の初日ではありません（日付順に隙間なく並べる）"
            ),
            Limits36Error::HistoryNotAgreementMonth { index } => {
                write!(f, "history[{index}].start が協定月の初日ではありません")
            }
            Limits36Error::HistoryNotBefore { index } => {
                write!(f, "history[{index}].start が months の最初の月以後です")
            }
            Limits36Error::DuplicateHistory { index } => {
                write!(
                    f,
                    "history[{index}].start と同じ月が、history に2回あります"
                )
            }
            Limits36Error::DateOutOfRange => {
                f.write_str("協定月の初日が、日付の範囲（1900〜2200年）の外です")
            }
            Limits36Error::MismatchedStartDay => f.write_str("月の期間の起算日がそろっていません"),
            Limits36Error::NoProgress => f.write_str("協定月の数え方が前に進みません"),
            Limits36Error::Overflow => f.write_str("分の合計が大きすぎます"),
        }
    }
}

impl std::error::Error for Limits36Error {}

/// 限度の項目を読む（ないなら `None`）。0〜`max` の外は範囲外。
fn limit(
    value: Option<i64>,
    field: &'static str,
    max: i64,
    expected: &'static str,
) -> Result<Option<i64>, Limits36Error> {
    match value {
        Some(minutes) if !(0..=max).contains(&minutes) => {
            Err(Limits36Error::AgreementOutOfRange { field, expected })
        }
        other => Ok(other),
    }
}

fn required(value: Option<i64>, field: &'static str) -> Result<i64, Limits36Error> {
    value.ok_or(Limits36Error::MissingLimit { field })
}

impl Agreement {
    /// 協定を検査する（`docs/spec/validation.md` の手順2: 範囲と、種類ごとの必須の項目）。
    ///
    /// 起算月は1〜12。限度は0以上で、法定の最大（2700・21600・5999・43200分・6回）を超えたら範囲外。
    /// 項目があれば、その種類で使わないものも範囲を見る。
    ///
    /// # Errors
    ///
    /// 範囲外は [`Limits36Error::AgreementOutOfRange`]、種類で必須の項目がなければ
    /// [`Limits36Error::MissingLimit`]。
    pub fn validate(&self) -> Result<ValidAgreement, Limits36Error> {
        let year_start_month = u32::try_from(self.year_start_month)
            .ok()
            .filter(|month| (1..=12).contains(month))
            .ok_or(Limits36Error::AgreementOutOfRange {
                field: "year_start_month",
                expected: "1〜12",
            })?;
        let monthly = limit(
            self.monthly_limit_minutes,
            "monthly_limit_minutes",
            MONTHLY_OVERTIME_LIMIT_MINUTES,
            "0〜2700",
        )?;
        let annual = limit(
            self.annual_limit_minutes,
            "annual_limit_minutes",
            ANNUAL_OVERTIME_LIMIT_MINUTES,
            "0〜21600",
        )?;
        let special_monthly_total = limit(
            self.special_monthly_total_limit_minutes,
            "special_monthly_total_limit_minutes",
            SPECIAL_MONTHLY_TOTAL_MAX_MINUTES,
            "0〜5999",
        )?;
        let special_annual = limit(
            self.special_annual_limit_minutes,
            "special_annual_limit_minutes",
            SPECIAL_ANNUAL_MAX_MINUTES,
            "0〜43200",
        )?;
        let special_months = limit(
            self.special_months_max,
            "special_months_max",
            SPECIAL_MONTHS_MAX,
            "0〜6",
        )?;
        let terms = match self.kind {
            AgreementKind::None => Terms::None,
            AgreementKind::General => Terms::General {
                monthly: required(monthly, "monthly_limit_minutes")?,
                annual: required(annual, "annual_limit_minutes")?,
            },
            AgreementKind::Special => Terms::Special {
                monthly: required(monthly, "monthly_limit_minutes")?,
                annual: required(annual, "annual_limit_minutes")?,
                special_monthly_total: required(
                    special_monthly_total,
                    "special_monthly_total_limit_minutes",
                )?,
                special_annual: required(special_annual, "special_annual_limit_minutes")?,
                special_months: required(special_months, "special_months_max")?,
            },
        };
        Ok(ValidAgreement {
            year_start_month,
            terms,
        })
    }
}

/// 協定月1つの分（入力の `months` の1件）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonthHours {
    /// 協定月の初日（日は1〜28で、全部の月で同じ）。
    pub start: Date,
    /// O: 法定時間外の分（法定休日労働を含まない。60時間超を含む）。
    pub overtime: i64,
    /// H: 法定休日労働の分。
    pub holiday: i64,
}

impl MonthHours {
    /// 区分の結果の月の期間1つから作る。O は `overtime`、H は `holiday`（区分の結果で
    /// 互いに重ならない。法定外休日の労働は、日・週の法定時間を超えた分だけが O に入っている）。
    ///
    /// # Errors
    ///
    /// 期間の初日が日付の型の範囲の外なら [`Limits36Error::DateOutOfRange`]。
    pub fn from_period(period: &PeriodResult) -> Result<MonthHours, Limits36Error> {
        Ok(MonthHours {
            start: period
                .start
                .first_day()
                .map_err(|_| Limits36Error::DateOutOfRange)?,
            overtime: period.overtime,
            holiday: period.holiday,
        })
    }

    /// T＝O＋H。
    ///
    /// # Errors
    ///
    /// 和が `i64` を超えれば [`Limits36Error::Overflow`]。
    pub fn total(&self) -> Result<i64, Limits36Error> {
        self.overtime
            .checked_add(self.holiday)
            .ok_or(Limits36Error::Overflow)
    }
}

/// 区分の結果の月の期間から、`first` から `last` まで隙間なく協定月を並べる。
/// 実労働のない月（区分の結果に期間がない月）は O も H も0。範囲の外の期間は使わない。
///
/// # Errors
///
/// `first`・`last`・期間の起算日がそろっていなければ [`Limits36Error::MismatchedStartDay`]、
/// 初日が日付の型の範囲の外なら [`Limits36Error::DateOutOfRange`]。
pub fn months_from_periods(
    periods: &[PeriodResult],
    first: MonthPeriod,
    last: MonthPeriod,
) -> Result<Vec<MonthHours>, Limits36Error> {
    let start_day = first.start_day();
    if last.start_day() != start_day || periods.iter().any(|p| p.start.start_day() != start_day) {
        return Err(Limits36Error::MismatchedStartDay);
    }
    let by_period: BTreeMap<MonthPeriod, &PeriodResult> = periods
        .iter()
        .map(|period| (period.start, period))
        .collect();
    let mut months = Vec::new();
    let mut current = first;
    while current <= last {
        let start = current
            .first_day()
            .map_err(|_| Limits36Error::DateOutOfRange)?;
        let (overtime, holiday) = by_period
            .get(&current)
            .map_or((0, 0), |period| (period.overtime, period.holiday));
        months.push(MonthHours {
            start,
            overtime,
            holiday,
        });
        let next = current
            .next_first_day()
            .map_err(|_| Limits36Error::DateOutOfRange)?;
        let following = MonthPeriod::containing(next, start_day);
        // 暦の計算が正しければ、次の期間は必ず後ろにある。前に進まないなら、無限に続けず誤りで止める。
        if following <= current {
            return Err(Limits36Error::NoProgress);
        }
        current = following;
    }
    Ok(months)
}

/// `months` より前の協定月の T（入力の `history` の1件）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoryMonth {
    /// 協定月の初日。
    pub start: Date,
    /// T（O＋H）の分。
    pub total: i64,
}

/// 判定の結果のコード。並び（宣言の順、同じコードは `n` の昇順）が出力の順。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Finding {
    /// 協定なしで、O または H がある。
    NoAgreement,
    /// 一般条項だけの協定で、O が45時間（2700分）を超えた。
    MonthlyOvertimeExceeded,
    /// O は45時間以下だが、協定の月の限度を超えた（発動月ではない）。
    GeneralMonthlyLimitExceeded,
    /// 一般条項だけの協定で、協定年の ΣO が360時間（21600分）を超えた（超えた月から協定年の終わりまで毎月）。
    AnnualOvertimeExceeded,
    /// 一般条項だけの協定で、ΣO は360時間以下だが、協定の年の限度を超えた。
    GeneralAnnualLimitExceeded,
    /// 特別条項の発動月の数が、協定の回数を超えた（超えた発動月だけ）。
    SpecialCountExceeded,
    /// 発動月の T が、特別条項の月の上限を超えた。
    SpecialMonthlyTotalExceeded,
    /// 協定年の ΣO が、特別条項の年の上限を超えた。
    SpecialAnnualExceeded,
    /// T が100時間（6000分）に達した（100時間未満でなければならない）。
    MonthlyTotalReached100h,
    /// 直近 n か月の T の合計が `4800 × n` 分を超えた（平均80時間超）。
    MultiMonthSumExceeded {
        /// 月数（2〜6）。
        n: u32,
    },
    /// 直近 n か月のうち、データも履歴もない月があり、判定できない（0分とはみなさない）。
    UnknownHistory {
        /// 月数（2〜6）。
        n: u32,
    },
}

impl Finding {
    /// 出力の `code` に出す文字列。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Finding::NoAgreement => "NoAgreement",
            Finding::MonthlyOvertimeExceeded => "MonthlyOvertimeExceeded",
            Finding::GeneralMonthlyLimitExceeded => "GeneralMonthlyLimitExceeded",
            Finding::AnnualOvertimeExceeded => "AnnualOvertimeExceeded",
            Finding::GeneralAnnualLimitExceeded => "GeneralAnnualLimitExceeded",
            Finding::SpecialCountExceeded => "SpecialCountExceeded",
            Finding::SpecialMonthlyTotalExceeded => "SpecialMonthlyTotalExceeded",
            Finding::SpecialAnnualExceeded => "SpecialAnnualExceeded",
            Finding::MonthlyTotalReached100h => "MonthlyTotalReached100h",
            Finding::MultiMonthSumExceeded { .. } => "MultiMonthSumExceeded",
            Finding::UnknownHistory { .. } => "UnknownHistory",
        }
    }

    /// 月数 n（`MultiMonthSumExceeded` と `UnknownHistory` だけ）。
    #[must_use]
    pub const fn n(self) -> Option<u32> {
        match self {
            Finding::MultiMonthSumExceeded { n } | Finding::UnknownHistory { n } => Some(n),
            _ => None,
        }
    }

    /// 根拠の条文。
    #[must_use]
    pub const fn provenances(self) -> &'static [Provenance] {
        match self {
            Finding::NoAgreement
            | Finding::GeneralMonthlyLimitExceeded
            | Finding::GeneralAnnualLimitExceeded => &[LSA_36_1],
            Finding::MonthlyOvertimeExceeded | Finding::AnnualOvertimeExceeded => &[LSA_36_4],
            Finding::SpecialCountExceeded
            | Finding::SpecialMonthlyTotalExceeded
            | Finding::SpecialAnnualExceeded => &[LSA_36_5],
            Finding::MonthlyTotalReached100h => &[LSA_36_6_ITEM2],
            Finding::MultiMonthSumExceeded { .. } | Finding::UnknownHistory { .. } => {
                &[LSA_36_6_ITEM3]
            }
        }
    }
}

/// 判定の注意。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Limits36Notice {
    /// 入力された自社の T だけを判定した。月100時間未満と2〜6か月平均80時間は、副業・兼業先や
    /// 転職前の時間も通算して適用される（労働基準法第38条第1項、厚生労働省『改正労働基準法に関するQ&A』2-32）。
    /// 通算した値を渡すのは呼び出し側。
    OwnEmployerOnly,
}

impl Limits36Notice {
    /// 出力の `notices` に出す文字列。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Limits36Notice::OwnEmployerOnly => "OwnEmployerOnly",
        }
    }
}

/// 協定月1つの判定（出力の `months` の1件）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonthJudgement {
    /// 協定月の初日。
    pub start: Date,
    /// T＝O＋H。
    pub total: i64,
    /// その月までの直近 n か月（n＝2〜6の順）の T の合計。月も履歴も足りなければ `None`。
    pub sums: [Option<i64>; 5],
    /// 協定年の始めからその月までの、O が2700分を超えた月（発動月）の数。
    pub special_count: u32,
    /// 協定年の始めからの ΣO。
    pub year_overtime: i64,
    /// 協定年の最初の月から `months` にあるか。偽なら `year_overtime` と `special_count` は下限値。
    pub year_overtime_known: bool,
    /// 判定の結果（コードの順、同じコードは `n` の昇順）。
    pub findings: Vec<Finding>,
}

impl MonthJudgement {
    /// 直近 `n` か月（2〜6）の T の合計。範囲外の `n` と、足りない月があるときは `None`。
    #[must_use]
    pub fn sum(&self, n: u32) -> Option<i64> {
        let index = usize::try_from(n.checked_sub(2)?).ok()?;
        self.sums.get(index).copied().flatten()
    }
}

/// 判定の結果（出力 `kintai.limits36.v1` に1対1で対応する）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits36Result {
    /// 協定月ごと（入力の `months` の順）。
    pub months: Vec<MonthJudgement>,
    /// 注意（常に [`Limits36Notice::OwnEmployerOnly`] を含む）。
    pub notices: Vec<Limits36Notice>,
}

/// 月の通し番号（西暦の年×12＋月−1）。
fn month_index(date: Date) -> i64 {
    i64::from(date.year()) * 12 + i64::from(date.month()) - 1
}

/// 協定月 `index` の分の範囲を見る（手順2）。O と H は0以上で、合わせて44640分以下。
///
/// [`judge`] も同じ検査をする。日付を読む前に範囲を見たいとき（コマンドライン）に使う。
///
/// # Errors
///
/// 負なら [`Limits36Error::MinutesOutOfRange`]、合計が44640分を超えれば
/// [`Limits36Error::MonthTotalTooLarge`]。
pub fn check_month_minutes(index: usize, overtime: i64, holiday: i64) -> Result<(), Limits36Error> {
    for (field, minutes) in [("overtime", overtime), ("holiday", holiday)] {
        if minutes < 0 {
            return Err(Limits36Error::MinutesOutOfRange {
                list: "months",
                index,
                field,
            });
        }
    }
    let within = overtime
        .checked_add(holiday)
        .is_some_and(|total| total <= MONTH_MINUTES_MAX);
    if within {
        Ok(())
    } else {
        Err(Limits36Error::MonthTotalTooLarge { index })
    }
}

/// 履歴 `index` の T の範囲（0〜44640分）を見る（手順2）。
///
/// # Errors
///
/// 範囲外なら [`Limits36Error::MinutesOutOfRange`]。
pub fn check_history_minutes(index: usize, total: i64) -> Result<(), Limits36Error> {
    if (0..=MONTH_MINUTES_MAX).contains(&total) {
        Ok(())
    } else {
        Err(Limits36Error::MinutesOutOfRange {
            list: "history",
            index,
            field: "total",
        })
    }
}

/// 手順2: 分の範囲。
fn check_minutes(months: &[MonthHours], history: &[HistoryMonth]) -> Result<(), Limits36Error> {
    for (index, month) in months.iter().enumerate() {
        check_month_minutes(index, month.overtime, month.holiday)?;
    }
    for (index, entry) in history.iter().enumerate() {
        check_history_minutes(index, entry.total)?;
    }
    Ok(())
}

/// 手順4: `months` の日は1〜28で、日付順に隙間なく、日が全部同じ。
fn check_months(months: &[MonthHours]) -> Result<(), Limits36Error> {
    let mut previous: Option<&MonthHours> = None;
    for (index, month) in months.iter().enumerate() {
        if !(1..=28).contains(&month.start.day()) {
            return Err(Limits36Error::MonthStartDay { index });
        }
        let follows = previous.is_none_or(|before| {
            month.start.day() == before.start.day()
                && month_index(month.start) == month_index(before.start) + 1
        });
        if !follows {
            return Err(Limits36Error::NotConsecutive { index });
        }
        previous = Some(month);
    }
    Ok(())
}

/// 手順4: `history` の検査。月の通し番号ごとの T を返す。
///
/// `months` が空のときは、日が1〜28であることだけを見る（履歴は使わない）。
fn check_history(
    history: &[HistoryMonth],
    first: Option<Date>,
) -> Result<BTreeMap<i64, i64>, Limits36Error> {
    let mut totals = BTreeMap::new();
    for (index, entry) in history.iter().enumerate() {
        let Some(first) = first else {
            if !(1..=28).contains(&entry.start.day()) {
                return Err(Limits36Error::HistoryNotAgreementMonth { index });
            }
            continue;
        };
        if entry.start.day() != first.day() {
            return Err(Limits36Error::HistoryNotAgreementMonth { index });
        }
        let key = month_index(entry.start);
        if key >= month_index(first) {
            return Err(Limits36Error::HistoryNotBefore { index });
        }
        if totals.insert(key, entry.total).is_some() {
            return Err(Limits36Error::DuplicateHistory { index });
        }
    }
    Ok(totals)
}

/// 協定月 `index`（`months` の中の位置）までの直近 n か月（n＝2〜6）の T の合計。
fn window_sums(
    index: usize,
    totals: &[i64],
    first: Date,
    history: &BTreeMap<i64, i64>,
) -> Result<[Option<i64>; 5], Limits36Error> {
    let here = i64::try_from(index).map_err(|_| Limits36Error::Overflow)?;
    let first_index = month_index(first);
    let mut sums = [None; 5];
    let mut running = Some(0_i64);
    for back in 0..6_i64 {
        let offset = here - back;
        let value = if offset >= 0 {
            usize::try_from(offset)
                .ok()
                .and_then(|at| totals.get(at))
                .copied()
        } else {
            history.get(&(first_index + offset)).copied()
        };
        running = match (running, value) {
            (Some(sum), Some(total)) => {
                Some(sum.checked_add(total).ok_or(Limits36Error::Overflow)?)
            }
            _ => None,
        };
        let slot = usize::try_from(back - 1)
            .ok()
            .and_then(|at| sums.get_mut(at));
        if let Some(slot) = slot {
            *slot = running;
        }
    }
    Ok(sums)
}

/// 協定年の累計（ΣO と発動月の数）。
#[derive(Clone, Copy, Debug, Default)]
struct YearState {
    overtime: i64,
    special_count: u32,
    known: bool,
}

/// 1か月の判定（コードの順に並べて返す）。
fn month_findings(
    terms: Terms,
    month: &MonthHours,
    total: i64,
    year: YearState,
    sums: &[Option<i64>; 5],
) -> Vec<Finding> {
    let overtime = month.overtime;
    let activated = overtime > MONTHLY_OVERTIME_LIMIT_MINUTES;
    let mut findings = Vec::new();
    match terms {
        Terms::None => {
            if overtime > 0 || month.holiday > 0 {
                findings.push(Finding::NoAgreement);
            }
        }
        Terms::General { monthly, annual } => {
            if activated {
                findings.push(Finding::MonthlyOvertimeExceeded);
            } else if overtime > monthly {
                findings.push(Finding::GeneralMonthlyLimitExceeded);
            }
            if year.overtime > ANNUAL_OVERTIME_LIMIT_MINUTES {
                findings.push(Finding::AnnualOvertimeExceeded);
            } else if year.overtime > annual {
                findings.push(Finding::GeneralAnnualLimitExceeded);
            }
        }
        Terms::Special {
            monthly,
            special_monthly_total,
            special_annual,
            special_months,
            ..
        } => {
            if !activated && overtime > monthly {
                findings.push(Finding::GeneralMonthlyLimitExceeded);
            }
            if activated && i64::from(year.special_count) > special_months {
                findings.push(Finding::SpecialCountExceeded);
            }
            if activated && total > special_monthly_total {
                findings.push(Finding::SpecialMonthlyTotalExceeded);
            }
            if year.overtime > special_annual {
                findings.push(Finding::SpecialAnnualExceeded);
            }
        }
    }
    if total >= MONTHLY_TOTAL_CAP_MINUTES {
        findings.push(Finding::MonthlyTotalReached100h);
    }
    for (n, sum) in WINDOW_MONTHS.into_iter().zip(sums) {
        match sum {
            Some(sum) if *sum > AVERAGE_CAP_MINUTES * i64::from(n) => {
                findings.push(Finding::MultiMonthSumExceeded { n });
            }
            Some(_) => {}
            None => findings.push(Finding::UnknownHistory { n }),
        }
    }
    findings.sort_unstable();
    findings
}

/// 36協定の上限を判定する（`docs/spec/limits36.md` の判定1〜9）。
///
/// `months` は協定月を日付順に隙間なく並べたもの（空でもよい）。`history` は `months` より前の
/// 協定月の T（歯抜けでよい。並びは問わない。使うのは直近5か月ぶんまで）。
/// 検査は、分の範囲（手順2）→ 協定月の日付の整合（手順4）→ 判定（手順5）の順。
///
/// - 協定年（起算月から協定月12個）が替わると、ΣO と発動月の数は0に戻る。直近 n か月の合計は戻らない。
/// - `months` の最初の月が協定年の途中なら、その年の ΣO と発動月の数は下限値で、
///   `year_overtime_known` が偽。下限値に同じ規則を当てるので、出るのは確実な違反だけ。
///
/// # Errors
///
/// 分の範囲外（O・H が負、O＋H が44640分超、T が0〜44640分の外）、協定月の日付の誤り
/// （日が29〜31、隙間、`history` の月の誤り・重複）、桁あふれ。
pub fn judge(
    agreement: &ValidAgreement,
    months: &[MonthHours],
    history: &[HistoryMonth],
) -> Result<Limits36Result, Limits36Error> {
    check_minutes(months, history)?;
    check_months(months)?;
    let first = months.first().map(|month| month.start);
    let history = check_history(history, first)?;
    let totals = months
        .iter()
        .map(MonthHours::total)
        .collect::<Result<Vec<i64>, Limits36Error>>()?;
    let mut judged = Vec::with_capacity(months.len());
    let mut year = YearState::default();
    for (index, (month, &total)) in months.iter().zip(&totals).enumerate() {
        let position = (month.start.month() + 12 - agreement.year_start_month) % 12;
        if index == 0 || position == 0 {
            year = YearState {
                overtime: 0,
                special_count: 0,
                known: position == 0,
            };
        }
        year.overtime = year
            .overtime
            .checked_add(month.overtime)
            .ok_or(Limits36Error::Overflow)?;
        if month.overtime > MONTHLY_OVERTIME_LIMIT_MINUTES {
            year.special_count = year
                .special_count
                .checked_add(1)
                .ok_or(Limits36Error::Overflow)?;
        }
        let sums = match first {
            Some(first) => window_sums(index, &totals, first, &history)?,
            None => [None; 5],
        };
        judged.push(MonthJudgement {
            start: month.start,
            total,
            sums,
            special_count: year.special_count,
            year_overtime: year.overtime,
            year_overtime_known: year.known,
            findings: month_findings(agreement.terms, month, total, year, &sums),
        });
    }
    Ok(Limits36Result {
        months: judged,
        notices: vec![Limits36Notice::OwnEmployerOnly],
    })
}
