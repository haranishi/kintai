//! 方針（`docs/spec/case-format.md` の `policy`）と、その検査。
//!
//! 検査の順序は `docs/spec/validation.md` のとおり。[`WorkPolicy::validate`] は、
//! 手順2（数値の範囲）を見てから、手順3（対応外の指定。表の上から順）を見る。
//! 手順1（JSONの形・型・知らない項目）と、手順2のうち列挙の値の読み取りは、
//! 入力を読む側（`kintai-cli`）が、この型を作る前に行う。

use std::fmt;

use kintai_core::{MonthStartDay, Weekday};

use crate::provenance::{
    LSA_32NO2, LSA_32NO3, LSA_32NO4, LSA_32NO5, LSA_35_1, LSA_35_2, LSA_36_6_ITEM1, LSA_36_11,
    LSA_37_1, LSA_37_4, LSA_38NO2, LSA_38NO3, LSA_38NO4, LSA_41_ITEM1, LSA_41_ITEM2, LSA_41_ITEM3,
    LSA_41NO2, LSA_60, LSA_61, LSA_66, LSA_SUPPL_139_1, LSA_SUPPL_140, LSA_SUPPL_141, Provenance,
    REG_18, REG_23, REG_25NO2,
};

/// 1日の法定労働時間（分）。労働基準法第32条第2項。
pub const DAILY_LIMIT_MINUTES: i64 = 480;
/// 週の法定労働時間（分）。労働基準法第32条第1項。
pub const WEEKLY_LIMIT_MINUTES: i64 = 2400;
/// 特例措置対象事業場の週の法定労働時間（分）。労働基準法施行規則第25条の2。
pub const WEEKLY_LIMIT_SPECIAL_MINUTES: i64 = 2640;
/// 月の期間の時間外のうち、60時間以下として数える上限（分）。労働基準法第37条第1項ただし書。
pub const OVER60_THRESHOLD_MINUTES: i64 = 3600;
/// 所定労働時間（1日）の上限（分）。
pub const MAX_SCHEDULED_MINUTES_PER_DAY: i64 = 480;

/// 労働時間制（`work_system`）。`Standard` 以外は対応外。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorkSystem {
    /// 通常の労働時間制（`standard`）。
    Standard,
    /// 1か月単位の変形労働時間制（`variable_monthly`）。
    VariableMonthly,
    /// 1年単位の変形労働時間制（`variable_yearly`）。
    VariableYearly,
    /// 1週間単位の非定型的変形労働時間制（`variable_weekly_irregular`）。
    VariableWeeklyIrregular,
    /// フレックスタイム制（`flex`）。
    Flex,
    /// 専門業務型・企画業務型の裁量労働制（`discretionary`）。
    Discretionary,
    /// 事業場外みなし労働時間制（`offsite_deemed`）。
    OffsiteDeemed,
    /// 管理監督者等（`managerial`）。
    Managerial,
    /// 農業・畜産・養蚕・水産の事業に従事する者（`agriculture_exempt`）。
    AgricultureExempt,
    /// 監視・断続的労働で許可を受けた者（`surveillance_intermittent`）。
    SurveillanceIntermittent,
    /// 宿直・日直勤務で許可を受けたもの（`night_duty_permit`）。
    NightDutyPermit,
    /// 高度プロフェッショナル制度（`highly_professional`）。
    HighlyProfessional,
}

impl WorkSystem {
    /// 対応外なら、そのコード。
    #[must_use]
    pub const fn unsupported(self) -> Option<UnsupportedCode> {
        match self {
            WorkSystem::Standard => None,
            WorkSystem::VariableMonthly => Some(UnsupportedCode::VariableMonthly),
            WorkSystem::VariableYearly => Some(UnsupportedCode::VariableYearly),
            WorkSystem::VariableWeeklyIrregular => Some(UnsupportedCode::VariableWeeklyIrregular),
            WorkSystem::Flex => Some(UnsupportedCode::Flex),
            WorkSystem::Discretionary => Some(UnsupportedCode::DiscretionaryDeemed),
            WorkSystem::OffsiteDeemed => Some(UnsupportedCode::OffsiteDeemed),
            WorkSystem::Managerial => Some(UnsupportedCode::Managerial),
            WorkSystem::AgricultureExempt => Some(UnsupportedCode::AgricultureExempt),
            WorkSystem::SurveillanceIntermittent => Some(UnsupportedCode::SurveillanceIntermittent),
            WorkSystem::NightDutyPermit => Some(UnsupportedCode::NightDutyPermit),
            WorkSystem::HighlyProfessional => Some(UnsupportedCode::HighlyProfessional),
        }
    }
}

/// 従業員の区分（`worker_kind`）。`Standard` 以外は対応外。
///
/// 一般の建設と、鹿児島・沖縄の砂糖製造は、2024-04-01 から一般の上限がすべて適用されるので、
/// 区分を持たない（`standard` として計算する）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorkerKind {
    /// 一般（`standard`）。
    Standard,
    /// 18歳未満の年少者（`minor`）。
    Minor,
    /// 妊産婦（`pregnant_or_postpartum`）。
    PregnantOrPostpartum,
    /// 新たな技術・商品・役務の研究開発業務（`research_development`）。
    ResearchDevelopment,
    /// 自動車運転の業務（`driver`）。
    Driver,
    /// 医業に従事する医師（`physician`）。
    Physician,
    /// 建設の事業のうち、災害時の復旧・復興の事業（`disaster_recovery_construction`）。
    DisasterRecoveryConstruction,
    /// 坑内労働その他の健康上特に有害な業務（`hazardous_work`）。
    HazardousWork,
}

impl WorkerKind {
    /// 対応外なら、そのコード。
    #[must_use]
    pub const fn unsupported(self) -> Option<UnsupportedCode> {
        match self {
            WorkerKind::Standard => None,
            WorkerKind::Minor => Some(UnsupportedCode::Minor),
            WorkerKind::PregnantOrPostpartum => Some(UnsupportedCode::PregnantOrPostpartum),
            WorkerKind::ResearchDevelopment => Some(UnsupportedCode::ResearchDevelopment),
            WorkerKind::Driver => Some(UnsupportedCode::AutomobileDriver),
            WorkerKind::Physician => Some(UnsupportedCode::Physician),
            WorkerKind::DisasterRecoveryConstruction => {
                Some(UnsupportedCode::DisasterRecoveryConstruction)
            }
            WorkerKind::HazardousWork => Some(UnsupportedCode::HazardousWork),
        }
    }
}

/// 休日の制度（`holiday_system`）。`Weekly` 以外は対応外。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HolidaySystem {
    /// 毎週1回の休日（`weekly`）。
    Weekly,
    /// 4週4日の変形週休制（`four_weeks`）。
    FourWeeks,
    /// 継続24時間の休日の例外（番方編成の交替制。`continuous_rotation`）。
    ContinuousRotation,
}

impl HolidaySystem {
    /// 対応外なら、そのコード。
    #[must_use]
    pub const fn unsupported(self) -> Option<UnsupportedCode> {
        match self {
            HolidaySystem::Weekly => None,
            HolidaySystem::FourWeeks => Some(UnsupportedCode::FourWeekHolidays),
            HolidaySystem::ContinuousRotation => Some(UnsupportedCode::ContinuousRotation),
        }
    }
}

/// 深夜の時間帯（`night_window`）。午後11時〜午前6時は対応外。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NightWindow {
    /// 午後10時〜午前5時（`22-05`）。
    From22To05,
    /// 午後11時〜午前6時（`23-06`。厚生労働大臣が地域・期間を定めた場合）。
    From23To06,
}

impl NightWindow {
    /// 対応外なら、そのコード。
    #[must_use]
    pub const fn unsupported(self) -> Option<UnsupportedCode> {
        match self {
            NightWindow::From22To05 => None,
            NightWindow::From23To06 => Some(UnsupportedCode::NightWindowOverride),
        }
    }
}

/// 曖昧さ（休日・週・月の境界をまたぐ勤務、打刻のそろっていない週）の扱い（`ambiguity`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AmbiguityMode {
    /// 確定集計を拒否する（`reject`。既定）。
    Reject,
    /// 区分の手順をそのまま当て、曖昧さの記録を付ける（`use_default`）。
    UseDefault,
}

/// 方針（検査の前の値）。範囲外の数値も持てる。[`WorkPolicy::validate`] で検査する。
///
/// `Option` の項目は、入力で省略（または `null`）されたもの。既定は `docs/spec/case-format.md` のとおり。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkPolicy {
    /// 労働時間制（必須）。
    pub work_system: WorkSystem,
    /// 1日の所定労働時間（分。必須。0〜480）。
    pub scheduled_minutes_per_day: i64,
    /// 週の起算の曜日（既定は日曜）。
    pub week_start: Option<Weekday>,
    /// 週の上限（分。2400（既定）か2640）。
    pub weekly_limit_minutes: Option<i64>,
    /// 週44時間の特例の対象であることを確かめたか（既定は偽）。
    pub special_44h_confirmed: Option<bool>,
    /// 法定休日の曜日（必須。ないと [`UnsupportedCode::LegalHolidayUnspecified`]）。
    pub legal_holiday_weekday: Option<Weekday>,
    /// 月の起算日（必須。1〜28）。
    pub month_start_day: i64,
    /// 曖昧さの扱い（既定は [`AmbiguityMode::Reject`]）。
    pub ambiguity: Option<AmbiguityMode>,
    /// 従業員の区分（既定は [`WorkerKind::Standard`]）。
    pub worker_kind: Option<WorkerKind>,
    /// 休日の制度（既定は [`HolidaySystem::Weekly`]）。
    pub holiday_system: Option<HolidaySystem>,
    /// 休日の振替（`holiday_swaps`）の指定の数。中身は見ない。1以上なら [`UnsupportedCode::HolidaySwap`]。
    pub holiday_swaps: usize,
    /// 深夜の時間帯（既定は [`NightWindow::From22To05`]）。
    pub night_window: Option<NightWindow>,
}

/// 検査済みの方針。[`WorkPolicy::validate`] だけが作る。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidPolicy {
    scheduled_minutes_per_day: i64,
    week_start: Weekday,
    weekly_limit_minutes: i64,
    legal_holiday_weekday: Weekday,
    month_start_day: MonthStartDay,
    ambiguity: AmbiguityMode,
}

impl ValidPolicy {
    /// 1日の所定労働時間（分。0〜480）。
    #[must_use]
    pub const fn scheduled_minutes_per_day(&self) -> i64 {
        self.scheduled_minutes_per_day
    }

    /// 週の起算の曜日。
    #[must_use]
    pub const fn week_start(&self) -> Weekday {
        self.week_start
    }

    /// 週の上限（分。2400か2640）。
    #[must_use]
    pub const fn weekly_limit_minutes(&self) -> i64 {
        self.weekly_limit_minutes
    }

    /// 法定休日の曜日。
    #[must_use]
    pub const fn legal_holiday_weekday(&self) -> Weekday {
        self.legal_holiday_weekday
    }

    /// 月の起算日。
    #[must_use]
    pub const fn month_start_day(&self) -> MonthStartDay {
        self.month_start_day
    }

    /// 曖昧さの扱い。
    #[must_use]
    pub const fn ambiguity(&self) -> AmbiguityMode {
        self.ambiguity
    }
}

/// 対応外の働き方のコード（`docs/spec/unsupported.md` の表）。終了コードは3。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnsupportedCode {
    /// 1か月単位の変形労働時間制。
    VariableMonthly,
    /// 1年単位の変形労働時間制。
    VariableYearly,
    /// 1週間単位の非定型的変形労働時間制。
    VariableWeeklyIrregular,
    /// フレックスタイム制。
    Flex,
    /// 専門業務型・企画業務型の裁量労働制。
    DiscretionaryDeemed,
    /// 事業場外みなし労働時間制。
    OffsiteDeemed,
    /// 監督・管理の地位にある者、機密の事務を取り扱う者。
    Managerial,
    /// 農業・畜産・養蚕・水産の事業に従事する者。
    AgricultureExempt,
    /// 監視・断続的労働で、行政官庁の許可を受けた者。
    SurveillanceIntermittent,
    /// 宿直・日直勤務で、許可を受けたもの。
    NightDutyPermit,
    /// 高度プロフェッショナル制度。
    HighlyProfessional,
    /// 18歳未満の年少者。
    Minor,
    /// 妊産婦の請求による制限。
    PregnantOrPostpartum,
    /// 自動車運転の業務。
    AutomobileDriver,
    /// 医業に従事する医師。
    Physician,
    /// 建設の事業のうち、災害時の復旧・復興の事業。
    DisasterRecoveryConstruction,
    /// 坑内労働その他の健康上特に有害な業務。
    HazardousWork,
    /// 新たな技術・商品・役務の研究開発業務。
    ResearchDevelopment,
    /// 法定休日を特定していない方針。
    LegalHolidayUnspecified,
    /// 4週4日の変形週休制。
    FourWeekHolidays,
    /// 継続24時間の休日の例外（番方編成の交替制）。
    ContinuousRotation,
    /// 休日の振替。
    HolidaySwap,
    /// 深夜の時間帯を午後11時〜午前6時とする指定。
    NightWindowOverride,
    /// 週44時間を、確認なしに選んだ方針。
    Weekly44WithoutConfirmation,
    /// 60時間超と36協定で、月の起算日が違う方針（コマンドラインの方針ファイルだけで判定する）。
    MonthStartDayMismatch,
}

impl UnsupportedCode {
    /// すべてのコード（`docs/spec/unsupported.md` の表の順）。
    pub const ALL: [UnsupportedCode; 25] = [
        UnsupportedCode::VariableMonthly,
        UnsupportedCode::VariableYearly,
        UnsupportedCode::VariableWeeklyIrregular,
        UnsupportedCode::Flex,
        UnsupportedCode::DiscretionaryDeemed,
        UnsupportedCode::OffsiteDeemed,
        UnsupportedCode::Managerial,
        UnsupportedCode::AgricultureExempt,
        UnsupportedCode::SurveillanceIntermittent,
        UnsupportedCode::NightDutyPermit,
        UnsupportedCode::HighlyProfessional,
        UnsupportedCode::Minor,
        UnsupportedCode::PregnantOrPostpartum,
        UnsupportedCode::AutomobileDriver,
        UnsupportedCode::Physician,
        UnsupportedCode::DisasterRecoveryConstruction,
        UnsupportedCode::HazardousWork,
        UnsupportedCode::ResearchDevelopment,
        UnsupportedCode::LegalHolidayUnspecified,
        UnsupportedCode::FourWeekHolidays,
        UnsupportedCode::ContinuousRotation,
        UnsupportedCode::HolidaySwap,
        UnsupportedCode::NightWindowOverride,
        UnsupportedCode::Weekly44WithoutConfirmation,
        UnsupportedCode::MonthStartDayMismatch,
    ];

    /// エラーの `code` に出す文字列。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            UnsupportedCode::VariableMonthly => "VariableMonthly",
            UnsupportedCode::VariableYearly => "VariableYearly",
            UnsupportedCode::VariableWeeklyIrregular => "VariableWeeklyIrregular",
            UnsupportedCode::Flex => "Flex",
            UnsupportedCode::DiscretionaryDeemed => "DiscretionaryDeemed",
            UnsupportedCode::OffsiteDeemed => "OffsiteDeemed",
            UnsupportedCode::Managerial => "Managerial",
            UnsupportedCode::AgricultureExempt => "AgricultureExempt",
            UnsupportedCode::SurveillanceIntermittent => "SurveillanceIntermittent",
            UnsupportedCode::NightDutyPermit => "NightDutyPermit",
            UnsupportedCode::HighlyProfessional => "HighlyProfessional",
            UnsupportedCode::Minor => "Minor",
            UnsupportedCode::PregnantOrPostpartum => "PregnantOrPostpartum",
            UnsupportedCode::AutomobileDriver => "AutomobileDriver",
            UnsupportedCode::Physician => "Physician",
            UnsupportedCode::DisasterRecoveryConstruction => "DisasterRecoveryConstruction",
            UnsupportedCode::HazardousWork => "HazardousWork",
            UnsupportedCode::ResearchDevelopment => "ResearchDevelopment",
            UnsupportedCode::LegalHolidayUnspecified => "LegalHolidayUnspecified",
            UnsupportedCode::FourWeekHolidays => "FourWeekHolidays",
            UnsupportedCode::ContinuousRotation => "ContinuousRotation",
            UnsupportedCode::HolidaySwap => "HolidaySwap",
            UnsupportedCode::NightWindowOverride => "NightWindowOverride",
            UnsupportedCode::Weekly44WithoutConfirmation => "Weekly44WithoutConfirmation",
            UnsupportedCode::MonthStartDayMismatch => "MonthStartDayMismatch",
        }
    }

    /// 働き方の名前。
    #[must_use]
    pub const fn work_style(self) -> &'static str {
        match self {
            UnsupportedCode::VariableMonthly => "1か月単位の変形労働時間制",
            UnsupportedCode::VariableYearly => "1年単位の変形労働時間制",
            UnsupportedCode::VariableWeeklyIrregular => "1週間単位の非定型的変形労働時間制",
            UnsupportedCode::Flex => "フレックスタイム制",
            UnsupportedCode::DiscretionaryDeemed => "専門業務型・企画業務型の裁量労働制",
            UnsupportedCode::OffsiteDeemed => "事業場外みなし労働時間制",
            UnsupportedCode::Managerial => "監督・管理の地位にある者、機密の事務を取り扱う者",
            UnsupportedCode::AgricultureExempt => "農業・畜産・養蚕・水産の事業に従事する者",
            UnsupportedCode::SurveillanceIntermittent => {
                "監視・断続的労働で、行政官庁の許可を受けた者"
            }
            UnsupportedCode::NightDutyPermit => "宿直・日直勤務で、許可を受けたもの",
            UnsupportedCode::HighlyProfessional => "高度プロフェッショナル制度",
            UnsupportedCode::Minor => "18歳未満の年少者",
            UnsupportedCode::PregnantOrPostpartum => "妊産婦の請求による制限",
            UnsupportedCode::AutomobileDriver => "自動車運転の業務",
            UnsupportedCode::Physician => "医業に従事する医師",
            UnsupportedCode::DisasterRecoveryConstruction => {
                "建設の事業のうち、災害時の復旧・復興の事業"
            }
            UnsupportedCode::HazardousWork => "坑内労働その他の健康上特に有害な業務",
            UnsupportedCode::ResearchDevelopment => "新たな技術・商品・役務の研究開発業務",
            UnsupportedCode::LegalHolidayUnspecified => "法定休日を特定していない方針",
            UnsupportedCode::FourWeekHolidays => "4週4日の変形週休制",
            UnsupportedCode::ContinuousRotation => "継続24時間の休日の例外（番方編成の交替制）",
            UnsupportedCode::HolidaySwap => "休日の振替",
            UnsupportedCode::NightWindowOverride => "深夜の時間帯を午後11時〜午前6時とする指定",
            UnsupportedCode::Weekly44WithoutConfirmation => "週44時間を、確認なしに選んだ方針",
            UnsupportedCode::MonthStartDayMismatch => "60時間超と36協定で、月の起算日が違う方針",
        }
    }

    /// 通常制として計算すると、どこが違ってしまうか。
    #[must_use]
    pub const fn difference(self) -> &'static str {
        match self {
            UnsupportedCode::VariableMonthly => "日・週の枠が、定めた労働時間に変わる",
            UnsupportedCode::VariableYearly => {
                "日・週の枠が、定めた労働時間に変わる。対象期間が3か月を超えるときは、時間外の上限（月42時間・年320時間）も変わる"
            }
            UnsupportedCode::VariableWeeklyIrregular => "1日の枠が10時間になる",
            UnsupportedCode::Flex => "清算期間の総枠で時間外を判定する",
            UnsupportedCode::DiscretionaryDeemed | UnsupportedCode::OffsiteDeemed => {
                "みなし時間で数える"
            }
            UnsupportedCode::Managerial => {
                "労働時間・休憩・休日の規定の適用外。深夜の規定は適用される"
            }
            UnsupportedCode::AgricultureExempt => "労働時間・休憩・休日の規定の適用外",
            UnsupportedCode::SurveillanceIntermittent => {
                "許可の範囲で、労働時間・休憩・休日の規定の適用外"
            }
            UnsupportedCode::NightDutyPermit => "許可の範囲で、労働時間等の規定の適用外",
            UnsupportedCode::HighlyProfessional => "労働時間・休憩・休日・深夜の規定の適用外",
            UnsupportedCode::Minor => "時間外・休日・深夜の制限が別にある",
            UnsupportedCode::PregnantOrPostpartum => "請求により時間外・休日・深夜が制限される",
            UnsupportedCode::AutomobileDriver => "36協定の上限が別になる（年960時間など）",
            UnsupportedCode::Physician => "36協定の上限が別になる",
            UnsupportedCode::DisasterRecoveryConstruction => {
                "月100時間未満・2〜6か月平均80時間の規制が適用されない"
            }
            UnsupportedCode::HazardousWork => "1日の時間外が2時間までに制限される",
            UnsupportedCode::ResearchDevelopment => "36協定の上限の一部が適用されない",
            UnsupportedCode::LegalHolidayUnspecified => {
                "週の中でどの休日が法定休日かが、働いた日で変わる"
            }
            UnsupportedCode::FourWeekHolidays => "4週間の起算日と、割増対象日の選定が要る",
            UnsupportedCode::ContinuousRotation => "暦日でなく継続24時間で休日を数える",
            UnsupportedCode::HolidaySwap => {
                "振替で勤務日になった元の法定休日の労働は、休日労働でなく平日の労働として数える"
            }
            UnsupportedCode::NightWindowOverride => "深夜の区間が変わる",
            UnsupportedCode::Weekly44WithoutConfirmation => "対象事業・10人未満の確認が要る",
            UnsupportedCode::MonthStartDayMismatch => "この版は両者に同じ起算日しか使えない",
        }
    }

    /// 根拠の条文（`docs/spec/unsupported.md` の「根拠」の欄のうち、法令の条項）。
    #[must_use]
    pub const fn provenances(self) -> &'static [Provenance] {
        match self {
            UnsupportedCode::VariableMonthly => &[LSA_32NO2],
            UnsupportedCode::VariableYearly => &[LSA_32NO4],
            UnsupportedCode::VariableWeeklyIrregular => &[LSA_32NO5],
            UnsupportedCode::Flex => &[LSA_32NO3],
            UnsupportedCode::DiscretionaryDeemed => &[LSA_38NO3, LSA_38NO4],
            UnsupportedCode::OffsiteDeemed => &[LSA_38NO2],
            UnsupportedCode::Managerial => &[LSA_41_ITEM2],
            UnsupportedCode::AgricultureExempt => &[LSA_41_ITEM1],
            UnsupportedCode::SurveillanceIntermittent => &[LSA_41_ITEM3],
            UnsupportedCode::NightDutyPermit => &[LSA_41_ITEM3, REG_23],
            UnsupportedCode::HighlyProfessional => &[LSA_41NO2],
            UnsupportedCode::Minor => &[LSA_60, LSA_61],
            UnsupportedCode::PregnantOrPostpartum => &[LSA_66],
            UnsupportedCode::AutomobileDriver => &[LSA_SUPPL_140],
            UnsupportedCode::Physician => &[LSA_SUPPL_141],
            UnsupportedCode::DisasterRecoveryConstruction => &[LSA_SUPPL_139_1],
            UnsupportedCode::HazardousWork => &[LSA_36_6_ITEM1, REG_18],
            UnsupportedCode::ResearchDevelopment => &[LSA_36_11],
            UnsupportedCode::LegalHolidayUnspecified
            | UnsupportedCode::ContinuousRotation
            | UnsupportedCode::HolidaySwap => &[LSA_35_1],
            UnsupportedCode::FourWeekHolidays => &[LSA_35_2],
            UnsupportedCode::NightWindowOverride => &[LSA_37_4],
            UnsupportedCode::Weekly44WithoutConfirmation => &[REG_25NO2],
            UnsupportedCode::MonthStartDayMismatch => &[LSA_37_1],
        }
    }

    /// 根拠の補足（条文だけでは足りないもの。通達・公的資料・調査の記録）。なければ空。
    #[must_use]
    pub const fn basis_note(self) -> &'static str {
        match self {
            UnsupportedCode::LegalHolidayUnspecified => "扱いは調査Aの記録",
            UnsupportedCode::ContinuousRotation => "継続24時間の例外は通達による（調査Aの記録）",
            UnsupportedCode::HolidaySwap => "厚生労働省『時間外・休日労働と割増賃金』",
            UnsupportedCode::MonthStartDayMismatch => "起算日は平成21年基発0529001号、Q&A 2-27",
            _ => "",
        }
    }

    /// 根拠の条文をつないだ表記（例: `労働基準法第38条の3、労働基準法第38条の4`）。
    #[must_use]
    pub fn citation(self) -> String {
        let mut text = self
            .provenances()
            .iter()
            .map(Provenance::citation)
            .collect::<Vec<_>>()
            .join("、");
        let note = self.basis_note();
        if !note.is_empty() {
            text.push('（');
            text.push_str(note);
            text.push('）');
        }
        text
    }
}

impl fmt::Display for UnsupportedCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "対応外の働き方です: {}（根拠: {}）。{}ため、通常の労働時間制として計算しません",
            self.work_style(),
            self.citation(),
            self.difference()
        )
    }
}

/// 方針の検査の結果のうち、誤り。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PolicyError {
    /// 不正な入力（終了コード2）。値が範囲外。
    Invalid {
        /// 項目の名前（例: `month_start_day`）。
        field: &'static str,
        /// 受け付ける値の説明。
        expected: &'static str,
    },
    /// 対応外（終了コード3）。
    Unsupported(UnsupportedCode),
}

impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PolicyError::Invalid { field, expected } => {
                write!(f, "policy.{field} の値が範囲外です（{expected}）")
            }
            PolicyError::Unsupported(code) => write!(f, "{code}"),
        }
    }
}

impl std::error::Error for PolicyError {}

impl WorkPolicy {
    /// 方針を検査する。
    ///
    /// 順序は `docs/spec/validation.md` のとおり。先に数値の範囲（手順2）を見て、
    /// 次に対応外の指定（手順3）を表の上から順に見る（`work_system`、`worker_kind`、
    /// `holiday_system`、`holiday_swaps`、`night_window`、`legal_holiday_weekday`、
    /// `weekly_limit_minutes`）。最初に当たったものを返す。
    ///
    /// # Errors
    ///
    /// 範囲外の数値は [`PolicyError::Invalid`]、対応外の指定は [`PolicyError::Unsupported`]。
    pub fn validate(&self) -> Result<ValidPolicy, PolicyError> {
        // 手順2: 数値の範囲
        if !(0..=MAX_SCHEDULED_MINUTES_PER_DAY).contains(&self.scheduled_minutes_per_day) {
            return Err(PolicyError::Invalid {
                field: "scheduled_minutes_per_day",
                expected: "0〜480",
            });
        }
        let month_start_day = u32::try_from(self.month_start_day)
            .ok()
            .and_then(|day| MonthStartDay::new(day).ok())
            .ok_or(PolicyError::Invalid {
                field: "month_start_day",
                expected: "1〜28",
            })?;
        let weekly_limit_minutes = self.weekly_limit_minutes.unwrap_or(WEEKLY_LIMIT_MINUTES);
        if weekly_limit_minutes != WEEKLY_LIMIT_MINUTES
            && weekly_limit_minutes != WEEKLY_LIMIT_SPECIAL_MINUTES
        {
            return Err(PolicyError::Invalid {
                field: "weekly_limit_minutes",
                expected: "2400 か 2640",
            });
        }

        // 手順3: 対応外の指定（表の上から順）
        let unsupported = [
            self.work_system.unsupported(),
            self.worker_kind
                .unwrap_or(WorkerKind::Standard)
                .unsupported(),
            self.holiday_system
                .unwrap_or(HolidaySystem::Weekly)
                .unsupported(),
            (self.holiday_swaps > 0).then_some(UnsupportedCode::HolidaySwap),
            self.night_window
                .unwrap_or(NightWindow::From22To05)
                .unsupported(),
        ];
        if let Some(code) = unsupported.into_iter().flatten().next() {
            return Err(PolicyError::Unsupported(code));
        }
        let Some(legal_holiday_weekday) = self.legal_holiday_weekday else {
            return Err(PolicyError::Unsupported(
                UnsupportedCode::LegalHolidayUnspecified,
            ));
        };
        if weekly_limit_minutes == WEEKLY_LIMIT_SPECIAL_MINUTES
            && self.special_44h_confirmed != Some(true)
        {
            return Err(PolicyError::Unsupported(
                UnsupportedCode::Weekly44WithoutConfirmation,
            ));
        }

        Ok(ValidPolicy {
            scheduled_minutes_per_day: self.scheduled_minutes_per_day,
            week_start: self.week_start.unwrap_or(Weekday::Sunday),
            weekly_limit_minutes,
            legal_holiday_weekday,
            month_start_day,
            ambiguity: self.ambiguity.unwrap_or(AmbiguityMode::Reject),
        })
    }
}
