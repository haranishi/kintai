//! 国民の祝日（`docs/spec/holidays.md`）。
//!
//! 国民の祝日は、労働基準法の法定休日ではない。祝日に働かせても、それだけでは休日の割増の対象に
//! ならない（法定休日は方針で曜日を指定する）。区分の計算（[`classify()`](crate::classify())）は祝日を使わない。
//! 使うのは、説明（`explain`）と所定休日の表示、将来の欠勤の判定。
//!
//! - 2020〜2027年は、内閣府の公式CSVの表（[`OFFICIAL_HOLIDAYS`]）を使う（[`HolidayStatus::Official`]）。
//! - 2028〜2099年は、祝日法2条・3条の規則と春分日・秋分日の近似式から生成する（[`HolidayStatus::Estimated`]）。
//!   近似式は、2050年までを国立天文台の予想表で確かめた。2051年以降は未検証で、
//!   [`HolidayNotice::EquinoxUnverified`] を付ける。
//! - 2020年より前と2099年より後は [`HolidayStatus::OutOfRange`]。
//!
//! 3条による休日（振替休日と国民の休日）は、2条の祝日として扱わず、再帰して計算しない。
//! 土曜が祝日のときの振替はない。
//!
//! ```
//! use kintai_jp_labor::holidays::{HolidayKind, HolidayStatus, holiday_on};
//!
//! let lookup = holiday_on("2026-09-22".parse().unwrap());
//! assert_eq!(lookup.status, HolidayStatus::Official);
//! let holiday = lookup.holiday.unwrap();
//! assert_eq!((holiday.name, holiday.kind), ("休日", HolidayKind::Citizens));
//!
//! let estimated = holiday_on("2030-01-01".parse().unwrap());
//! assert_eq!(estimated.status, HolidayStatus::Estimated);
//! assert_eq!(estimated.holiday.unwrap().name, "元日");
//! ```

use std::collections::{BTreeMap, BTreeSet};

use kintai_core::{Date, Weekday};

use crate::provenance::{CONFIRMED_ON, Law, Provenance};

/// 国民の祝日に関する法律（2020-01-01 施行の版）。
pub const NATIONAL_HOLIDAYS_ACT: Law = Law {
    name: "国民の祝日に関する法律",
    law_id: "323AC1000000178",
    revision_id: "323AC1000000178_20200101_430AC1000000057",
    enforced_on: "2020-01-01",
    url: "https://laws.e-gov.go.jp/law/323AC1000000178",
};

const fn holidays_act(article: &'static str, summary: &'static str) -> Provenance {
    Provenance {
        law: NATIONAL_HOLIDAYS_ACT,
        article,
        summary,
        confirmed_on: CONFIRMED_ON,
    }
}

/// 国民の祝日に関する法律第2条: 国民の祝日（16日）。
pub const HOLIDAYS_ACT_2: Provenance =
    holidays_act("第2条", "国民の祝日（元日から勤労感謝の日までの16日）");
/// 国民の祝日に関する法律第3条第2項: 振替休日。
pub const HOLIDAYS_ACT_3_2: Provenance = holidays_act(
    "第3条第2項",
    "国民の祝日が日曜日に当たるときは、その日後においてその日に最も近い国民の祝日でない日を休日とする",
);
/// 国民の祝日に関する法律第3条第3項: 国民の休日。
pub const HOLIDAYS_ACT_3_3: Provenance = holidays_act(
    "第3条第3項",
    "その前日及び翌日が国民の祝日である日（国民の祝日でない日に限る）は、休日とする",
);

/// 出力に付ける出典の1行。
pub const SOURCE_NOTE: &str = "出典: 内閣府「国民の祝日について」の公式CSV（https://www8.cao.go.jp/chosei/shukujitsu/syukujitsu.csv、2026-10-15取得、公共データ利用規約（第1.0版））を加工（UTF-8に変換し、2020〜2027年の行に休日の種別を付けた）。2028年以降は規則と近似式による推定";

/// 公式の表がある最初の年。これより前は [`HolidayStatus::OutOfRange`]。
pub const OFFICIAL_FIRST_YEAR: i32 = 2020;
/// 公式の表がある最後の年。
pub const OFFICIAL_LAST_YEAR: i32 = 2027;
/// 規則から推定する最後の年。これより後は [`HolidayStatus::OutOfRange`]。
pub const ESTIMATED_LAST_YEAR: i32 = 2099;
/// 春分日・秋分日の近似式を、国立天文台の予想表で確かめた最後の年。
pub const EQUINOX_VERIFIED_LAST_YEAR: i32 = 2050;

/// 祝日の種別。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HolidayKind {
    /// 2条の国民の祝日（特別措置法で移した2020・2021年の海の日・スポーツの日・山の日を含む）。
    Statutory,
    /// 3条2項の休日（振替休日）。名前は「休日」。
    Substitute,
    /// 3条3項の休日（国民の休日）。名前は「休日」。
    Citizens,
}

impl HolidayKind {
    /// 出力に出す文字列。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            HolidayKind::Statutory => "Statutory",
            HolidayKind::Substitute => "Substitute",
            HolidayKind::Citizens => "Citizens",
        }
    }

    /// 根拠の条文。
    #[must_use]
    pub const fn provenance(self) -> Provenance {
        match self {
            HolidayKind::Statutory => HOLIDAYS_ACT_2,
            HolidayKind::Substitute => HOLIDAYS_ACT_3_2,
            HolidayKind::Citizens => HOLIDAYS_ACT_3_3,
        }
    }
}

/// 祝日の表の状態（年ごと）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HolidayStatus {
    /// 内閣府の公式CSVの表（2020〜2027年）。
    Official,
    /// 規則と近似式から生成した推定（2028〜2099年）。確定の暦ではない。
    Estimated,
    /// 表も推定もない年（2020年より前と2099年より後）。
    OutOfRange,
}

impl HolidayStatus {
    /// 出力に出す文字列。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            HolidayStatus::Official => "Official",
            HolidayStatus::Estimated => "Estimated",
            HolidayStatus::OutOfRange => "OutOfRange",
        }
    }
}

/// 祝日の注意。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HolidayNotice {
    /// 春分日・秋分日の近似式が、公的な表で確かめられていない年（2051〜2099年）。
    EquinoxUnverified,
}

impl HolidayNotice {
    /// 出力に出す文字列。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            HolidayNotice::EquinoxUnverified => "EquinoxUnverified",
        }
    }
}

/// 祝日・休日の1日。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Holiday {
    /// 日付。
    pub date: Date,
    /// 名前（2条の祝日の名前。3条の休日は「休日」。公式CSVの名称と同じ）。
    pub name: &'static str,
    /// 種別。
    pub kind: HolidayKind,
}

/// 公式の表の1行。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OfficialHoliday {
    /// 年。
    pub year: i32,
    /// 月。
    pub month: u32,
    /// 日。
    pub day: u32,
    /// 公式CSVの名称（3条の休日は「休日」）。
    pub name: &'static str,
    /// 種別（公式CSVにはない。3条の規則で付けた）。
    pub kind: HolidayKind,
}

const fn official(
    year: i32,
    month: u32,
    day: u32,
    name: &'static str,
    kind: HolidayKind,
) -> OfficialHoliday {
    OfficialHoliday {
        year,
        month,
        day,
        name,
        kind,
    }
}

/// 内閣府「国民の祝日について」の公式CSVの、2020〜2027年の全143行（日付順）。
///
/// - 出典: <https://www8.cao.go.jp/chosei/shukujitsu/syukujitsu.csv>（2026-10-15 取得）。
///   利用条件は公共データ利用規約（第1.0版）。
/// - 元のファイル（`Shift_JIS`）の SHA-256: `cec37a743c96995cdb9cb52b685c9003634682a9b0e1a640a6b9b96881fe964a`
/// - UTF-8 に変換した `docs/legal/sources/syukujitsu-2026-10-15.csv` の SHA-256:
///   `6a4e90c95de4d143336501f40379a17bb14e44d0cb588da89c8c1e58e8ffec1d`
/// - 加工: 2020年以降の行を抜き出し、名称「休日」の行に、3条2項（振替休日）か3条3項（国民の休日）かの
///   種別を付けた（公式CSVには種別の列がない）。日付と名称は変えていない。
pub const OFFICIAL_HOLIDAYS: [OfficialHoliday; 143] = [
    official(2020, 1, 1, "元日", HolidayKind::Statutory),
    official(2020, 1, 13, "成人の日", HolidayKind::Statutory),
    official(2020, 2, 11, "建国記念の日", HolidayKind::Statutory),
    official(2020, 2, 23, "天皇誕生日", HolidayKind::Statutory),
    official(2020, 2, 24, "休日", HolidayKind::Substitute),
    official(2020, 3, 20, "春分の日", HolidayKind::Statutory),
    official(2020, 4, 29, "昭和の日", HolidayKind::Statutory),
    official(2020, 5, 3, "憲法記念日", HolidayKind::Statutory),
    official(2020, 5, 4, "みどりの日", HolidayKind::Statutory),
    official(2020, 5, 5, "こどもの日", HolidayKind::Statutory),
    official(2020, 5, 6, "休日", HolidayKind::Substitute),
    official(2020, 7, 23, "海の日", HolidayKind::Statutory),
    official(2020, 7, 24, "スポーツの日", HolidayKind::Statutory),
    official(2020, 8, 10, "山の日", HolidayKind::Statutory),
    official(2020, 9, 21, "敬老の日", HolidayKind::Statutory),
    official(2020, 9, 22, "秋分の日", HolidayKind::Statutory),
    official(2020, 11, 3, "文化の日", HolidayKind::Statutory),
    official(2020, 11, 23, "勤労感謝の日", HolidayKind::Statutory),
    official(2021, 1, 1, "元日", HolidayKind::Statutory),
    official(2021, 1, 11, "成人の日", HolidayKind::Statutory),
    official(2021, 2, 11, "建国記念の日", HolidayKind::Statutory),
    official(2021, 2, 23, "天皇誕生日", HolidayKind::Statutory),
    official(2021, 3, 20, "春分の日", HolidayKind::Statutory),
    official(2021, 4, 29, "昭和の日", HolidayKind::Statutory),
    official(2021, 5, 3, "憲法記念日", HolidayKind::Statutory),
    official(2021, 5, 4, "みどりの日", HolidayKind::Statutory),
    official(2021, 5, 5, "こどもの日", HolidayKind::Statutory),
    official(2021, 7, 22, "海の日", HolidayKind::Statutory),
    official(2021, 7, 23, "スポーツの日", HolidayKind::Statutory),
    official(2021, 8, 8, "山の日", HolidayKind::Statutory),
    official(2021, 8, 9, "休日", HolidayKind::Substitute),
    official(2021, 9, 20, "敬老の日", HolidayKind::Statutory),
    official(2021, 9, 23, "秋分の日", HolidayKind::Statutory),
    official(2021, 11, 3, "文化の日", HolidayKind::Statutory),
    official(2021, 11, 23, "勤労感謝の日", HolidayKind::Statutory),
    official(2022, 1, 1, "元日", HolidayKind::Statutory),
    official(2022, 1, 10, "成人の日", HolidayKind::Statutory),
    official(2022, 2, 11, "建国記念の日", HolidayKind::Statutory),
    official(2022, 2, 23, "天皇誕生日", HolidayKind::Statutory),
    official(2022, 3, 21, "春分の日", HolidayKind::Statutory),
    official(2022, 4, 29, "昭和の日", HolidayKind::Statutory),
    official(2022, 5, 3, "憲法記念日", HolidayKind::Statutory),
    official(2022, 5, 4, "みどりの日", HolidayKind::Statutory),
    official(2022, 5, 5, "こどもの日", HolidayKind::Statutory),
    official(2022, 7, 18, "海の日", HolidayKind::Statutory),
    official(2022, 8, 11, "山の日", HolidayKind::Statutory),
    official(2022, 9, 19, "敬老の日", HolidayKind::Statutory),
    official(2022, 9, 23, "秋分の日", HolidayKind::Statutory),
    official(2022, 10, 10, "スポーツの日", HolidayKind::Statutory),
    official(2022, 11, 3, "文化の日", HolidayKind::Statutory),
    official(2022, 11, 23, "勤労感謝の日", HolidayKind::Statutory),
    official(2023, 1, 1, "元日", HolidayKind::Statutory),
    official(2023, 1, 2, "休日", HolidayKind::Substitute),
    official(2023, 1, 9, "成人の日", HolidayKind::Statutory),
    official(2023, 2, 11, "建国記念の日", HolidayKind::Statutory),
    official(2023, 2, 23, "天皇誕生日", HolidayKind::Statutory),
    official(2023, 3, 21, "春分の日", HolidayKind::Statutory),
    official(2023, 4, 29, "昭和の日", HolidayKind::Statutory),
    official(2023, 5, 3, "憲法記念日", HolidayKind::Statutory),
    official(2023, 5, 4, "みどりの日", HolidayKind::Statutory),
    official(2023, 5, 5, "こどもの日", HolidayKind::Statutory),
    official(2023, 7, 17, "海の日", HolidayKind::Statutory),
    official(2023, 8, 11, "山の日", HolidayKind::Statutory),
    official(2023, 9, 18, "敬老の日", HolidayKind::Statutory),
    official(2023, 9, 23, "秋分の日", HolidayKind::Statutory),
    official(2023, 10, 9, "スポーツの日", HolidayKind::Statutory),
    official(2023, 11, 3, "文化の日", HolidayKind::Statutory),
    official(2023, 11, 23, "勤労感謝の日", HolidayKind::Statutory),
    official(2024, 1, 1, "元日", HolidayKind::Statutory),
    official(2024, 1, 8, "成人の日", HolidayKind::Statutory),
    official(2024, 2, 11, "建国記念の日", HolidayKind::Statutory),
    official(2024, 2, 12, "休日", HolidayKind::Substitute),
    official(2024, 2, 23, "天皇誕生日", HolidayKind::Statutory),
    official(2024, 3, 20, "春分の日", HolidayKind::Statutory),
    official(2024, 4, 29, "昭和の日", HolidayKind::Statutory),
    official(2024, 5, 3, "憲法記念日", HolidayKind::Statutory),
    official(2024, 5, 4, "みどりの日", HolidayKind::Statutory),
    official(2024, 5, 5, "こどもの日", HolidayKind::Statutory),
    official(2024, 5, 6, "休日", HolidayKind::Substitute),
    official(2024, 7, 15, "海の日", HolidayKind::Statutory),
    official(2024, 8, 11, "山の日", HolidayKind::Statutory),
    official(2024, 8, 12, "休日", HolidayKind::Substitute),
    official(2024, 9, 16, "敬老の日", HolidayKind::Statutory),
    official(2024, 9, 22, "秋分の日", HolidayKind::Statutory),
    official(2024, 9, 23, "休日", HolidayKind::Substitute),
    official(2024, 10, 14, "スポーツの日", HolidayKind::Statutory),
    official(2024, 11, 3, "文化の日", HolidayKind::Statutory),
    official(2024, 11, 4, "休日", HolidayKind::Substitute),
    official(2024, 11, 23, "勤労感謝の日", HolidayKind::Statutory),
    official(2025, 1, 1, "元日", HolidayKind::Statutory),
    official(2025, 1, 13, "成人の日", HolidayKind::Statutory),
    official(2025, 2, 11, "建国記念の日", HolidayKind::Statutory),
    official(2025, 2, 23, "天皇誕生日", HolidayKind::Statutory),
    official(2025, 2, 24, "休日", HolidayKind::Substitute),
    official(2025, 3, 20, "春分の日", HolidayKind::Statutory),
    official(2025, 4, 29, "昭和の日", HolidayKind::Statutory),
    official(2025, 5, 3, "憲法記念日", HolidayKind::Statutory),
    official(2025, 5, 4, "みどりの日", HolidayKind::Statutory),
    official(2025, 5, 5, "こどもの日", HolidayKind::Statutory),
    official(2025, 5, 6, "休日", HolidayKind::Substitute),
    official(2025, 7, 21, "海の日", HolidayKind::Statutory),
    official(2025, 8, 11, "山の日", HolidayKind::Statutory),
    official(2025, 9, 15, "敬老の日", HolidayKind::Statutory),
    official(2025, 9, 23, "秋分の日", HolidayKind::Statutory),
    official(2025, 10, 13, "スポーツの日", HolidayKind::Statutory),
    official(2025, 11, 3, "文化の日", HolidayKind::Statutory),
    official(2025, 11, 23, "勤労感謝の日", HolidayKind::Statutory),
    official(2025, 11, 24, "休日", HolidayKind::Substitute),
    official(2026, 1, 1, "元日", HolidayKind::Statutory),
    official(2026, 1, 12, "成人の日", HolidayKind::Statutory),
    official(2026, 2, 11, "建国記念の日", HolidayKind::Statutory),
    official(2026, 2, 23, "天皇誕生日", HolidayKind::Statutory),
    official(2026, 3, 20, "春分の日", HolidayKind::Statutory),
    official(2026, 4, 29, "昭和の日", HolidayKind::Statutory),
    official(2026, 5, 3, "憲法記念日", HolidayKind::Statutory),
    official(2026, 5, 4, "みどりの日", HolidayKind::Statutory),
    official(2026, 5, 5, "こどもの日", HolidayKind::Statutory),
    official(2026, 5, 6, "休日", HolidayKind::Substitute),
    official(2026, 7, 20, "海の日", HolidayKind::Statutory),
    official(2026, 8, 11, "山の日", HolidayKind::Statutory),
    official(2026, 9, 21, "敬老の日", HolidayKind::Statutory),
    official(2026, 9, 22, "休日", HolidayKind::Citizens),
    official(2026, 9, 23, "秋分の日", HolidayKind::Statutory),
    official(2026, 10, 12, "スポーツの日", HolidayKind::Statutory),
    official(2026, 11, 3, "文化の日", HolidayKind::Statutory),
    official(2026, 11, 23, "勤労感謝の日", HolidayKind::Statutory),
    official(2027, 1, 1, "元日", HolidayKind::Statutory),
    official(2027, 1, 11, "成人の日", HolidayKind::Statutory),
    official(2027, 2, 11, "建国記念の日", HolidayKind::Statutory),
    official(2027, 2, 23, "天皇誕生日", HolidayKind::Statutory),
    official(2027, 3, 21, "春分の日", HolidayKind::Statutory),
    official(2027, 3, 22, "休日", HolidayKind::Substitute),
    official(2027, 4, 29, "昭和の日", HolidayKind::Statutory),
    official(2027, 5, 3, "憲法記念日", HolidayKind::Statutory),
    official(2027, 5, 4, "みどりの日", HolidayKind::Statutory),
    official(2027, 5, 5, "こどもの日", HolidayKind::Statutory),
    official(2027, 7, 19, "海の日", HolidayKind::Statutory),
    official(2027, 8, 11, "山の日", HolidayKind::Statutory),
    official(2027, 9, 20, "敬老の日", HolidayKind::Statutory),
    official(2027, 9, 23, "秋分の日", HolidayKind::Statutory),
    official(2027, 10, 11, "スポーツの日", HolidayKind::Statutory),
    official(2027, 11, 3, "文化の日", HolidayKind::Statutory),
    official(2027, 11, 23, "勤労感謝の日", HolidayKind::Statutory),
];

/// 春分日・秋分日の近似式の固定小数点の単位（10^-6）。
const EQUINOX_SCALE: i64 = 1_000_000;
/// 春分日の近似式の定数 20.8431（固定小数点）。
const VERNAL_BASE: i64 = 20_843_100;
/// 秋分日の近似式の定数 23.2488（固定小数点）。
const AUTUMNAL_BASE: i64 = 23_248_800;
/// 近似式の1年あたりの増分 0.242194（固定小数点）。
const EQUINOX_DRIFT: i64 = 242_194;

/// 近似式の日 `floor(base + 0.242194×(y−1980) − floor((y−1980)/4))`。整数の固定小数点で計算する
/// （係数は小数第6位までなので、10^6 倍すれば誤差なく整数になる）。2020〜2099年の外は `None`。
fn equinox_day(year: i32, base: i64) -> Option<u32> {
    if !(OFFICIAL_FIRST_YEAR..=ESTIMATED_LAST_YEAR).contains(&year) {
        return None;
    }
    let years = i64::from(year) - 1980;
    let fixed = base + EQUINOX_DRIFT * years - EQUINOX_SCALE * years.div_euclid(4);
    u32::try_from(fixed.div_euclid(EQUINOX_SCALE)).ok()
}

/// 春分日（近似式。2020〜2099年。外は `None`）。公式の日付は国立天文台が前年2月に官報で公表する。
#[must_use]
pub fn vernal_equinox(year: i32) -> Option<Date> {
    Date::from_ymd(year, 3, equinox_day(year, VERNAL_BASE)?).ok()
}

/// 秋分日（近似式。2020〜2099年。外は `None`）。
#[must_use]
pub fn autumnal_equinox(year: i32) -> Option<Date> {
    Date::from_ymd(year, 9, equinox_day(year, AUTUMNAL_BASE)?).ok()
}

/// `month` 月の第 `n` 月曜日。
fn nth_monday(year: i32, month: u32, n: u32) -> Option<Date> {
    let first = Date::from_ymd(year, month, 1).ok()?;
    let offset = Weekday::Monday.days_since(first.weekday()) + 7 * n.saturating_sub(1);
    first.checked_add_days(i64::from(offset)).ok()
}

/// 2条の国民の祝日（規則による。日付順。2020〜2099年の外は `None`）。
///
/// 2020年と2021年は、特別措置法（令和三年東京オリンピック競技大会・東京パラリンピック競技大会特別措置法
/// 第32条）により、海の日・スポーツの日・山の日が移った。その日付は公式CSVのとおり。
#[must_use]
pub fn statutory_holidays(year: i32) -> Option<Vec<Holiday>> {
    if !(OFFICIAL_FIRST_YEAR..=ESTIMATED_LAST_YEAR).contains(&year) {
        return None;
    }
    let ymd = |month: u32, day: u32| Date::from_ymd(year, month, day).ok();
    let (marine, sports, mountain) = match year {
        2020 => (ymd(7, 23)?, ymd(7, 24)?, ymd(8, 10)?),
        2021 => (ymd(7, 22)?, ymd(7, 23)?, ymd(8, 8)?),
        _ => (
            nth_monday(year, 7, 3)?,
            nth_monday(year, 10, 2)?,
            ymd(8, 11)?,
        ),
    };
    let mut list = [
        ("元日", ymd(1, 1)?),
        ("成人の日", nth_monday(year, 1, 2)?),
        ("建国記念の日", ymd(2, 11)?),
        ("天皇誕生日", ymd(2, 23)?),
        ("春分の日", vernal_equinox(year)?),
        ("昭和の日", ymd(4, 29)?),
        ("憲法記念日", ymd(5, 3)?),
        ("みどりの日", ymd(5, 4)?),
        ("こどもの日", ymd(5, 5)?),
        ("海の日", marine),
        ("山の日", mountain),
        ("敬老の日", nth_monday(year, 9, 3)?),
        ("秋分の日", autumnal_equinox(year)?),
        ("スポーツの日", sports),
        ("文化の日", ymd(11, 3)?),
        ("勤労感謝の日", ymd(11, 23)?),
    ];
    list.sort_by_key(|(_, date)| *date);
    Some(
        list.into_iter()
            .map(|(name, date)| Holiday {
                date,
                name,
                kind: HolidayKind::Statutory,
            })
            .collect(),
    )
}

/// 3条の休日を、2条の祝日の日付だけから求める（日付順）。名前は「休日」。
///
/// - 3条2項（振替休日）: 日曜の祝日の後で、最も近い、祝日でない日。
/// - 3条3項（国民の休日）: 前日と翌日が祝日で、自身は祝日でない日。
///
/// 求めた休日は祝日として扱わず、再帰しない（振替休日と祝日に挟まれた日は、国民の休日にならない）。
/// 同じ日が両方に当たれば、振替休日とする（2020〜2099年の規則の祝日では起きない）。
#[must_use]
pub fn article3_holidays(statutory: &[Date]) -> Vec<Holiday> {
    let set: BTreeSet<Date> = statutory.iter().copied().collect();
    let mut derived: BTreeMap<Date, HolidayKind> = BTreeMap::new();
    for &date in &set {
        if date.weekday() != Weekday::Sunday {
            continue;
        }
        let mut candidate = date.checked_add_days(1);
        while let Ok(day) = candidate {
            if !set.contains(&day) {
                break;
            }
            candidate = day.checked_add_days(1);
        }
        if let Ok(day) = candidate {
            derived.entry(day).or_insert(HolidayKind::Substitute);
        }
    }
    for &date in &set {
        let Ok(middle) = date.checked_add_days(1) else {
            continue;
        };
        let Ok(after) = middle.checked_add_days(1) else {
            continue;
        };
        if !set.contains(&middle) && set.contains(&after) {
            derived.entry(middle).or_insert(HolidayKind::Citizens);
        }
    }
    derived
        .into_iter()
        .map(|(date, kind)| Holiday {
            date,
            name: "休日",
            kind,
        })
        .collect()
}

/// 規則から、その年の祝日と3条の休日を生成する（日付順。2020〜2099年の外は `None`）。
///
/// 2020〜2027年については、公式CSVと全件一致することを試験で確かめている。
#[must_use]
pub fn generate(year: i32) -> Option<Vec<Holiday>> {
    let statutory = statutory_holidays(year)?;
    let dates: Vec<Date> = statutory.iter().map(|holiday| holiday.date).collect();
    let mut all = statutory;
    all.extend(
        article3_holidays(&dates)
            .into_iter()
            .filter(|holiday| holiday.date.year() == year),
    );
    all.sort_by_key(|holiday| holiday.date);
    Some(all)
}

/// その年の表の状態。
#[must_use]
pub fn status(year: i32) -> HolidayStatus {
    if (OFFICIAL_FIRST_YEAR..=OFFICIAL_LAST_YEAR).contains(&year) {
        HolidayStatus::Official
    } else if (OFFICIAL_LAST_YEAR + 1..=ESTIMATED_LAST_YEAR).contains(&year) {
        HolidayStatus::Estimated
    } else {
        HolidayStatus::OutOfRange
    }
}

/// その年の注意（2051〜2099年は [`HolidayNotice::EquinoxUnverified`]）。
#[must_use]
pub fn notices(year: i32) -> Vec<HolidayNotice> {
    if status(year) == HolidayStatus::Estimated && year > EQUINOX_VERIFIED_LAST_YEAR {
        vec![HolidayNotice::EquinoxUnverified]
    } else {
        Vec::new()
    }
}

/// 1年分の祝日と休日。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YearHolidays {
    /// 年。
    pub year: i32,
    /// 表の状態。
    pub status: HolidayStatus,
    /// 祝日と休日（日付順。範囲外の年は空）。
    pub holidays: Vec<Holiday>,
    /// 注意。
    pub notices: Vec<HolidayNotice>,
}

/// その年の祝日と休日。2020〜2027年は公式の表、2028〜2099年は規則による推定、ほかは空。
#[must_use]
pub fn holidays_of_year(year: i32) -> YearHolidays {
    let status = status(year);
    let holidays = match status {
        HolidayStatus::Official => OFFICIAL_HOLIDAYS
            .iter()
            .filter(|row| row.year == year)
            .filter_map(|row| {
                Some(Holiday {
                    date: Date::from_ymd(row.year, row.month, row.day).ok()?,
                    name: row.name,
                    kind: row.kind,
                })
            })
            .collect(),
        HolidayStatus::Estimated => generate(year).unwrap_or_default(),
        HolidayStatus::OutOfRange => Vec::new(),
    };
    YearHolidays {
        year,
        status,
        holidays,
        notices: notices(year),
    }
}

/// ある日の祝日の引き当て。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HolidayLookup {
    /// 日付。
    pub date: Date,
    /// その年の表の状態。
    pub status: HolidayStatus,
    /// 祝日か休日なら、その1日（範囲外の年は常に `None`）。
    pub holiday: Option<Holiday>,
    /// 注意。
    pub notices: Vec<HolidayNotice>,
}

/// その日が祝日・休日かを引く。法定休日の判定には使わない（法定休日は方針の曜日で決まる）。
#[must_use]
pub fn holiday_on(date: Date) -> HolidayLookup {
    let year = holidays_of_year(date.year());
    HolidayLookup {
        date,
        status: year.status,
        holiday: year
            .holidays
            .into_iter()
            .find(|holiday| holiday.date == date),
        notices: year.notices,
    }
}
